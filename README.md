# serum-crank

A performance and cost optimized [serum-dex crank](https://github.com/project-serum/serum-dex/tree/master/dex/crank) that allows combining multiple market cranking instructions into a single transaction, while concurrently generating the crank instructions allowing for increased throughput.
# Overview

This works by taking a list of markets which need to be cranked, and checking each market concurrently with the rest. Any markets which can be cranked during the given work loop iteration will be combined into a single transaction. If no markets are ready to be cranked, the work loop exits, otherwise it will crank all available markets.

## Transaction Size Limits

Depending on your configured settings, you may be able to crank up to 6 markets in a single transaction.
## Compiler Optimizations

The rustc compiler settings have been set to as optimized as they can be, so build times will generally be slower than normal.

## Global Allocator

The global allocator has been replaced with `jemalloc`, allowing the usage of `jemalloc` over the default which usually ends up being `malloc`.

# Docker

A docker image and compose file is included, which can be built locally providing you meet the following requirements:

* buildkit supported by your docker daemon to enable build caching
* Experimental features enabled in your docker daemon.
* *optional* - pigz to compress the exported docker image

To build the docker image run

```shell
$> make build-docker
```

After it is built and you have the configuration file filled out, you can run:

```shell
$> docker-compose up -d
```

If it is your first time building the docker image it will take awhile. On an i7-9750H w/ 32GB of memory first time builds take around 400 -> 600 seconds. Subsequent builds will usually take 50% of this time, so typically anywhere from 200 -> 300 seconds.

# CLI

To use the docker image requires a configuration file which is the primary use of the CLI. Although it can be used to run the crank service as well. To build the CLI run

```shell
$> make build-cli
$> ./crank --help
serum-crank 0.0.1
Solfarm
a performance optimized serum crank service

USAGE:
    crank [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --config <FILE>    sets the config file

SUBCOMMANDS:
    config    configuration management commands
    help      Prints this message or the help of the given subcommand(s)
    run       runs the serum crank

```

# Configuration

With the CLI built you can generate a config file located in the current working directory named `config.yaml` with the `config` commands:

```shell
---
http_rpc_url: "https://api.devnet.solana.com"
ws_rpc_url: "ws://api.devnet.solana.com"
key_path: ~/.config/solana/id.json
log_file: liquidator.log
debug_log: false
crank:
  # used to configure the markets to crank
  markets:
    # name of the market to crank
    - name: TULIP-USDC
      # the market account public key
      market_account: somekey
      # random coin wallet,doesnt really matter what you pick
      # we use an ATA account
      coin_wallet: somewallet
      # random pc wallet, as as above
      pc_wallet: some_pc_wallet
  # the serum dex program
  dex_program: 9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin
  # the amount of time in seconds to wait in between crank runs
  max_wait_for_events_delay: 60
  # the max number of accounts to include in a single crank
  # if you want to get up to 6 markets per tx, you will want to set this to 5
  num_accounts: 32
  # max events processed per worker
  events_per_worker: 5
  # max number of markets to crank in a single tx
  # if there are more markets to crank than this number
  # we chunk the markets to crank into groups of this number
  max_markets_per_tx: 6
```

# License

Based on code from Serum, so for their [click here](https://github.com/project-serum/serum-dex/blob/master/LICENSE)