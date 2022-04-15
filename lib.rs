// Sources
// https://paritytech.github.io/ink-docs/datastructures/mapping
//
// Run tests: cargo +nightly test -- --nocapture
//
// TODO
// - fee earnings are not handled here ... do it via eg. earnings: Balances

#![allow(non_snake_case)]
#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;
const PRECISION: u128 = 1_000_000; // Precision of 6 digits

#[ink::contract]
mod amm {
    #[cfg(not(feature = "ink-as-dependency"))]
    //use ink_storage::collections::HashMap;
    //use std::collections::HashMap;
    use ink_storage::traits::SpreadAllocate;

    type Balances = ink_storage::Mapping<AccountId, Balance>;

    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.

    #[derive(Default)]
    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Amm {
        totalShares: Balance,     // Stores the total amount of share issued for the pool
        totalToken1: Balance,     // Stores the amount of Token1 locked in the pool
        totalToken2: Balance,     // Stores the amount of Token2 locked in the pool
        shares: Balances,         // Stores the share holding of each provider
        token1Balances: Balances, // Stores the token1 balance of each user
        token2Balances: Balances, // Stores the token2 balance of each user
        fees: Balance,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Zero Liquidity
        ZeroLiquidity,
        /// Amount cannot be zero!
        ZeroAmount,
        /// Insufficient amount
        InsufficientAmount,
        /// Equivalent value of tokens not provided
        NonEquivalentValue,
        /// Asset value less than threshold for contribution!
        ThresholdNotReached,
        /// Share should be less than totalShare
        InvalidShare,
        /// Insufficient pool balance
        InsufficientLiquidity,
        /// Slippage tolerance exceeded
        SlippageExceeded,
    }

    impl Amm {
        fn validAmountCheck(&self, balances: &Balances, qty: Balance) -> Result<(), Error> {
            let caller = self.env().caller();
            let my_balance = balances.get(&caller).unwrap_or(0);

            match qty {
                0 => Err(Error::ZeroAmount),
                _ if qty > my_balance => Err(Error::InsufficientAmount),
                _ => Ok(()),
            }
        }

        fn getK(&self) -> Balance {
            self.totalToken1 * self.totalToken2
        }

        fn activePool(&self) -> Result<(), Error> {
            match self.getK() {
                0 => Err(Error::ZeroLiquidity),
                _ => Ok(()),
            }
        }

        /// Constructs a new AMM instance
        /// @param _fees: valid interval -> [0,1000]
        #[ink(constructor)]
        pub fn new(fees: Balance) -> Self {
            //Self {
            //fees: if fees > 1000 { 0 } else { fees },
            //..Default::default()
            //}
            ink_lang::utils::initialize_contract(|contract: &mut Self| {
                let caller = Self::env().caller();
                contract.shares.insert(&caller, &0);
                contract.token1Balances.insert(&caller, &0);
                contract.token2Balances.insert(&caller, &0);
                contract.fees = fees;
            })
        }
        #[ink(constructor)]
        pub fn default() -> Self {
            // Even though we're not explicitly initializing the `Mapping`,
            // we still need to call this
            ink_lang::utils::initialize_contract(|_| {})
        }

        #[ink(message)]
        pub fn faucet(&mut self, token1_amount: Balance, token2_amount: Balance) {
            let caller = self.env().caller();
            let token1_balance = self.token1Balances.get(&caller).unwrap_or(0);
            let token2_balance = self.token2Balances.get(&caller).unwrap_or(0);

            self.token1Balances
                .insert(caller, &(token1_balance + token1_amount));
            self.token2Balances
                .insert(caller, &(token2_balance + token2_amount));
        }

        #[ink(message)]
        pub fn getMyHoldings(&self) -> (Balance, Balance, Balance) {
            let caller = self.env().caller();
            let token1_balance = self.token1Balances.get(&caller).unwrap_or(0);
            let token2_balance = self.token2Balances.get(&caller).unwrap_or(0);
            let shares = self.shares.get(&caller).unwrap_or(0);

            (token1_balance, token2_balance, shares)
        }

        #[ink(message)]
        pub fn getPoolDetails(&self) -> (Balance, Balance, Balance, Balance) {
            (
                self.totalToken1,
                self.totalToken2,
                self.totalShares,
                self.fees,
            )
        }

        #[ink(message)]
        pub fn provide(
            &mut self,
            token1_amount: Balance,
            token2_amount: Balance,
        ) -> Result<Balance, Error> {
            self.validAmountCheck(&self.token1Balances, token1_amount)?;
            self.validAmountCheck(&self.token2Balances, token2_amount)?;

            let share = if self.totalShares == 0 {
                100 * super::PRECISION
            } else {
                let share1 = self.totalShares * token1_amount / self.totalToken1;
                let share2 = self.totalShares * token2_amount / self.totalToken2;

                if share1 != share2 {
                    return Err(Error::NonEquivalentValue);
                }
                share1
            };

            if share == 0 {
                return Err(Error::ThresholdNotReached);
            }

            let caller = self.env().caller();
            let token1_balance = self.token1Balances.get(&caller).unwrap();
            let token2_balance = self.token2Balances.get(&caller).unwrap();
            self.token1Balances
                .insert(caller, &(token1_balance - token1_amount));
            self.token2Balances
                .insert(caller, &(token2_balance - token2_amount));

            self.totalToken1 += token1_amount;
            self.totalToken2 += token2_amount;
            self.totalShares += share;

            let caller_share = self.shares.get(&caller).unwrap_or(0);

            self.shares.insert(caller, &(caller_share + share));
            Ok(share)
        }

        #[ink(message)]
        pub fn get_withdraw_estimate(&self, share: Balance) -> Result<(Balance, Balance), Error> {
            self.activePool()?;

            if share > self.totalShares {
                return Err(Error::InvalidShare);
            }

            let amountToken1 = share * self.totalToken1 / self.totalShares;
            let amountToken2 = share * self.totalToken2 / self.totalShares;
            Ok((amountToken1, amountToken2))
        }

        #[ink(message)]
        pub fn withdraw(&mut self, share: Balance) -> Result<(Balance, Balance), Error> {
            let caller = self.env().caller();
            self.validAmountCheck(&self.shares, share)?;

            let caller_share = self.shares.get(&caller).unwrap();
            let caller_token1_balance = self.token1Balances.get(&caller).unwrap();
            let caller_token2_balance = self.token2Balances.get(&caller).unwrap();

            let (token1_amount, token2_amount) = self.get_withdraw_estimate(share)?;
            self.shares.insert(caller, &(caller_share - share));
            self.totalShares -= share;
            self.totalToken1 -= token1_amount;
            self.totalToken2 -= token2_amount;

            self.token1Balances
                .insert(caller, &(caller_token1_balance + token1_amount));
            self.token2Balances
                .insert(caller, &(caller_token2_balance + token2_amount));
            Ok((token1_amount, token2_amount))
        }

        #[ink(message)]
        pub fn swap_token1_to_token2(&mut self, token1_amount: Balance, token2_min: Balance) -> Result<Balance, Error> {
            self.activePool()?;
            self.validAmountCheck(&self.token1Balances, token1_amount)?;
            let caller = self.env().caller();

            let fee = self.fees * token1_amount / 1000;
            let token1_w_fee = token1_amount - fee;

            let total_token1_after = self.totalToken1 + token1_w_fee;
            let total_token2_after = self.getK() / total_token1_after;

            // current total - calculated total after swap by K formula (x * y = K) ^^^
            // it means we won't get token2 amount related to rate BEFORE exchange
            // but related to rate AFTER exchange .... SLIPPAGE
            let token2_withdraw = self.totalToken2 - total_token2_after;

            // check slippage
            if token2_withdraw < token2_min {
                return Err(Error::SlippageExceeded);
            }

            self.totalToken1 = total_token1_after;
            self.totalToken2 = total_token2_after;

            let caller_token2_balance = self.token2Balances.get(caller).unwrap_or(0);
            self.token2Balances.insert(caller, &(caller_token2_balance + token2_withdraw));

            let caller_token1_balance = self.token1Balances.get(caller).unwrap();
            self.token1Balances.insert(caller, &(caller_token1_balance - token1_amount));

            Ok(token2_withdraw)
        }

        #[ink(message)]
        pub fn swap_token1_to_token2(&mut self, token1_amount: Balance, token2_min: Balance) -> Result<Balance, Error> {
            self.activePool()?;
            self.validAmountCheck(&self.token1Balances, token1_amount)?;
            let caller = self.env().caller();

            let fee = self.fees * token1_amount / 1000;
        }
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        /// Imports `ink_lang` so we can use `#[ink::test]`.
        use ink_lang as ink;

        #[ink::test]
        fn new_works() {
            let contract = Amm::new(0);
            assert_eq!(contract.getMyHoldings(), (0, 0, 0));
            assert_eq!(contract.getPoolDetails(), (0, 0, 0, 0));
        }

        #[ink::test]
        fn faucet_works() {
            let mut contract = Amm::new(0);
            contract.faucet(10, 20);
            assert_eq!(contract.getMyHoldings(), (10, 20, 0));
            assert_eq!(contract.getPoolDetails(), (0, 0, 0, 0));
        }

        #[ink::test]
        fn activePool_test() {
            let contract = Amm::new(0);
            let res = contract.activePool();
            assert_eq!(res, Err(Error::ZeroLiquidity));
        }

        #[ink::test]
        fn provide_test() {
            let mut contract = Amm::new(0);
            contract.faucet(100, 200);
            let share = contract.provide(10, 20).unwrap();
            assert_eq!(share, 100_000_000);
            assert_eq!(contract.getMyHoldings(), (90, 180, share));
            assert_eq!(contract.getPoolDetails(), (10, 20, share, 0));
        }

        #[ink::test]
        fn withdraw_test() {
            let mut contract = Amm::new(0);
            contract.faucet(100, 200);
            let share = contract.provide(10, 20).unwrap();
            assert_eq!(contract.withdraw(share / 2).unwrap(), (5, 10));
            assert_eq!(contract.getMyHoldings(), (95, 190, share/2));
            assert_eq!(contract.getPoolDetails(), (5, 10, share/2, 0));
        }

        #[ink::test]
        fn swap_token1_to_token2_test() {
            let mut contract = Amm::new(500); // 50%
            contract.faucet(200, 200);
            let share = contract.provide(100, 200).unwrap();
            assert_eq!(contract.getMyHoldings(), (100, 0, share));
            assert_eq!(contract.getPoolDetails(), (100, 200, share, 500));

            let _res = contract.swap_token1_to_token2(50, 10);
            // 50 token1 provided ... w/ fee (50%) it is 25
            // rate in pool is 1 token1 / 2 token2 ... so 25 token1 * 2 -> 50 token2
            // with slippage it will be 40 token2 (pool state/rate 125 / 160)
            assert_eq!(contract.getMyHoldings(), (50, 40, share));
            // token1 100 + 25 (given amount w/o fee), token2 200 - withdrawed 40
            assert_eq!(contract.getPoolDetails(), (125, 160, share, 500));
        }
    }
}
