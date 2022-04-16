# Automated Market Maker
Build an AMM on Polkadot using Ink.

Inspired by tutorial https://learn.figment.io/tutorials/build-polkadot-amm-using-ink.

### Faucet
On chat https://app.element.io/#/room/#jupiter-faucet-room:matrix.org write `!drip <Address>`

### Sources
 - https://paritytech.github.io/ink-docs/datastructures/mapping

### Tests
`cargo +nightly test -- --nocapture`

### Deploying
`cargo +nightly contract build --release`

### TODO
 - fee earnings are not handled here ... do it via eg. earnings: Balances
