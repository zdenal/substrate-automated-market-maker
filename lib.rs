// Sources
// https://paritytech.github.io/ink-docs/datastructures/mapping
//
// Run tests: cargo +nightly test -- --nocapture
//
// Deploying
// cargo +nightly contract build --release
// TODO
// - fee earnings are not handled here ... do it via eg. earnings: Balances

//#![allow(non_snake_case)]
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
        total_shares: Balance, // Stores the total amount of share issued for the pool
        token1_total: Balance, // Stores the amount of Token1 locked in the pool
        token2_total: Balance, // Stores the amount of Token2 locked in the pool
        shares: Balances,      // Stores the share holding of each provider
        token1_balances: Balances, // Stores the token1 balance of each user
        token2_balances: Balances, // Stores the token2 balance of each user
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
        fn valid_amount_check(&self, balances: &Balances, qty: Balance) -> Result<(), Error> {
            let caller = self.env().caller();
            let my_balance = balances.get(&caller).unwrap_or(0);

            match qty {
                0 => Err(Error::ZeroAmount),
                _ if qty > my_balance => Err(Error::InsufficientAmount),
                _ => Ok(()),
            }
        }

        fn get_k(&self) -> Balance {
            self.token1_total * self.token2_total
        }

        fn active_pool(&self) -> Result<(), Error> {
            match self.get_k() {
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
                contract.token1_balances.insert(&caller, &0);
                contract.token2_balances.insert(&caller, &0);
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
            let token1_balance = self.token1_balances.get(&caller).unwrap_or(0);
            let token2_balance = self.token2_balances.get(&caller).unwrap_or(0);

            self.token1_balances
                .insert(caller, &(token1_balance + token1_amount));
            self.token2_balances
                .insert(caller, &(token2_balance + token2_amount));
        }

        #[ink(message)]
        pub fn get_my_holdings(&self) -> (Balance, Balance, Balance) {
            let caller = self.env().caller();
            let token1_balance = self.token1_balances.get(&caller).unwrap_or(0);
            let token2_balance = self.token2_balances.get(&caller).unwrap_or(0);
            let shares = self.shares.get(&caller).unwrap_or(0);

            (token1_balance, token2_balance, shares)
        }

        #[ink(message)]
        pub fn get_pool_details(&self) -> (Balance, Balance, Balance, Balance) {
            (
                self.token1_total,
                self.token2_total,
                self.total_shares,
                self.fees,
            )
        }

        #[ink(message)]
        pub fn provide(
            &mut self,
            token1_amount: Balance,
            token2_amount: Balance,
        ) -> Result<Balance, Error> {
            self.valid_amount_check(&self.token1_balances, token1_amount)?;
            self.valid_amount_check(&self.token2_balances, token2_amount)?;

            let share = if self.total_shares == 0 {
                100 * super::PRECISION
            } else {
                let share1 = self.total_shares * token1_amount / self.token1_total;
                let share2 = self.total_shares * token2_amount / self.token2_total;

                if share1 != share2 {
                    return Err(Error::NonEquivalentValue);
                }
                share1
            };

            if share == 0 {
                return Err(Error::ThresholdNotReached);
            }

            let caller = self.env().caller();
            let token1_balance = self.token1_balances.get(&caller).unwrap();
            let token2_balance = self.token2_balances.get(&caller).unwrap();
            self.token1_balances
                .insert(caller, &(token1_balance - token1_amount));
            self.token2_balances
                .insert(caller, &(token2_balance - token2_amount));

            self.token1_total += token1_amount;
            self.token2_total += token2_amount;
            self.total_shares += share;

            let caller_share = self.shares.get(&caller).unwrap_or(0);

            self.shares.insert(caller, &(caller_share + share));
            Ok(share)
        }

        #[ink(message)]
        pub fn get_withdraw_estimate(&self, share: Balance) -> Result<(Balance, Balance), Error> {
            self.active_pool()?;

            if share > self.total_shares {
                return Err(Error::InvalidShare);
            }

            let token1_amount = share * self.token1_total / self.total_shares;
            let token2_amount = share * self.token2_total / self.total_shares;
            Ok((token1_amount, token2_amount))
        }

        #[ink(message)]
        pub fn withdraw(&mut self, share: Balance) -> Result<(Balance, Balance), Error> {
            let caller = self.env().caller();
            self.valid_amount_check(&self.shares, share)?;

            let caller_share = self.shares.get(&caller).unwrap();
            let caller_token1_balance = self.token1_balances.get(&caller).unwrap();
            let caller_token2_balance = self.token2_balances.get(&caller).unwrap();

            let (token1_amount, token2_amount) = self.get_withdraw_estimate(share)?;
            self.shares.insert(caller, &(caller_share - share));
            self.total_shares -= share;
            self.token1_total -= token1_amount;
            self.token2_total -= token2_amount;

            self.token1_balances
                .insert(caller, &(caller_token1_balance + token1_amount));
            self.token2_balances
                .insert(caller, &(caller_token2_balance + token2_amount));
            Ok((token1_amount, token2_amount))
        }

        #[ink(message)]
        pub fn swap_token1_to_token2(
            &mut self,
            token1_amount: Balance,
            token2_min: Balance,
        ) -> Result<Balance, Error> {
            self.active_pool()?;
            self.valid_amount_check(&self.token1_balances, token1_amount)?;
            if token1_amount >= self.token1_total {
                return Err(Error::InsufficientLiquidity);
            }
            let caller = self.env().caller();

            let fee = self.fees * token1_amount / 1000;
            let token1_w_fee = token1_amount - fee;

            let total_token1_after = self.token1_total + token1_w_fee;
            let total_token2_after = self.get_k() / total_token1_after;

            // current total - calculated total after swap by K formula (x * y = K) ^^^
            // it means we won't get token2 amount related to rate BEFORE exchange
            // but related to rate AFTER exchange .... SLIPPAGE
            let token2_withdraw = self.token2_total - total_token2_after;

            // check slippage
            if token2_withdraw < token2_min {
                return Err(Error::SlippageExceeded);
            }

            self.token1_total = total_token1_after;
            self.token2_total = total_token2_after;

            let caller_token2_balance = self.token2_balances.get(caller).unwrap_or(0);
            self.token2_balances
                .insert(caller, &(caller_token2_balance + token2_withdraw));

            let caller_token1_balance = self.token1_balances.get(caller).unwrap();
            self.token1_balances
                .insert(caller, &(caller_token1_balance - token1_amount));

            Ok(token2_withdraw)
        }

        #[ink(message)]
        pub fn swap_token2_to_token1(
            &mut self,
            token2_amount: Balance,
            token1_min: Balance,
        ) -> Result<Balance, Error> {
            self.active_pool()?;
            self.valid_amount_check(&self.token2_balances, token2_amount)?;
            if token2_amount >= self.token2_total {
                return Err(Error::InsufficientLiquidity);
            }
            let caller = self.env().caller();

            let fee = self.fees * token2_amount / 1000;
            let token2_w_fee = token2_amount - fee;

            let total_token2_after = self.token2_total + token2_w_fee;
            let total_token1_after = self.get_k() / total_token2_after;

            let token1_withdraw = self.token1_total - total_token1_after;

            // check slippage
            if token1_withdraw < token1_min {
                return Err(Error::SlippageExceeded);
            }

            self.token1_total = total_token1_after;
            self.token2_total = total_token2_after;

            let caller_token1_balance = self.token1_balances.get(caller).unwrap_or(0);
            self.token1_balances
                .insert(caller, &(caller_token1_balance + token1_withdraw));

            let caller_token2_balance = self.token2_balances.get(caller).unwrap();
            self.token2_balances
                .insert(caller, &(caller_token2_balance - token2_amount));

            Ok(token1_withdraw)
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
            assert_eq!(contract.get_my_holdings(), (0, 0, 0));
            assert_eq!(contract.get_pool_details(), (0, 0, 0, 0));
        }

        #[ink::test]
        fn faucet_works() {
            let mut contract = Amm::new(0);
            contract.faucet(10, 20);
            assert_eq!(contract.get_my_holdings(), (10, 20, 0));
            assert_eq!(contract.get_pool_details(), (0, 0, 0, 0));
        }

        #[ink::test]
        fn active_pool_test() {
            let contract = Amm::new(0);
            let res = contract.active_pool();
            assert_eq!(res, Err(Error::ZeroLiquidity));
        }

        #[ink::test]
        fn provide_test() {
            let mut contract = Amm::new(0);
            contract.faucet(100, 200);
            let share = contract.provide(10, 20).unwrap();
            assert_eq!(share, 100_000_000);
            assert_eq!(contract.get_my_holdings(), (90, 180, share));
            assert_eq!(contract.get_pool_details(), (10, 20, share, 0));
        }

        #[ink::test]
        fn withdraw_test() {
            let mut contract = Amm::new(0);
            contract.faucet(100, 200);
            let share = contract.provide(10, 20).unwrap();
            assert_eq!(contract.withdraw(share / 2).unwrap(), (5, 10));
            assert_eq!(contract.get_my_holdings(), (95, 190, share / 2));
            assert_eq!(contract.get_pool_details(), (5, 10, share / 2, 0));
        }

        #[ink::test]
        fn swap_token1_to_token2_test() {
            let mut contract = Amm::new(500); // 50%
            contract.faucet(200, 200);
            let share = contract.provide(100, 200).unwrap();
            assert_eq!(contract.get_my_holdings(), (100, 0, share));
            assert_eq!(contract.get_pool_details(), (100, 200, share, 500));

            let _res = contract.swap_token1_to_token2(50, 10);
            // 50 token1 provided ... w/ fee (50%) it is 25
            // rate in pool is 1 token1 / 2 token2 ... so 25 token1 * 2 -> 50 token2
            // with slippage it will be 40 token2 (pool state/rate 125 / 160)
            assert_eq!(contract.get_my_holdings(), (50, 40, share));
            // token1 100 + 25 (given amount w/o fee), token2 200 - withdrawed 40
            assert_eq!(contract.get_pool_details(), (125, 160, share, 500));
        }

        #[ink::test]
        fn swap_token2_to_token1_test() {
            let mut contract = Amm::new(0);
            contract.faucet(200, 200);
            let share = contract.provide(100, 100).unwrap();
            assert_eq!(contract.get_my_holdings(), (100, 100, share));
            assert_eq!(contract.get_pool_details(), (100, 100, share, 0));

            let _res = contract.swap_token2_to_token1(50, 20);
            println!("res: {:?}", _res);
            assert_eq!(contract.get_my_holdings(), (134, 50, share));
            // token2 100 + 50 (given amount), token2 66 - withdrawed 34
            // before K: 100 * 100 = 10_000
            // after K: 66 * 150 = 9_900 ....  Balance is u128 so it doesn't have decimal
            // precission. Correctly it should be 66.66666 * 150 = 10_000
            assert_eq!(contract.get_pool_details(), (66, 150, share, 0));
        }
    }
}
