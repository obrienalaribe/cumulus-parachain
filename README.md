# A Simple [Cumulus](https://github.com/paritytech/cumulus/)-less Parachain

## Build a collator

```
cd collator
cargo build --release
```

## Run with zombienet

```
cd .. # from the workspace root
# assumes polkadot is in PATH
zombienet test --provider native zombienet.zndsl
```
