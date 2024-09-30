# Bitvmx Instance Monitor


This process checks all BitVMX instances in a file and verifies them against a Bitcoin indexer to ensure that the transactions associated with the BitVMX instances are found in the blockchain.

![Explanation](draw.png)

## Installation
Clone the repository and initialize the submodules:
```bash
$ git clone --recurse-submodules git@github.com:FairgateLabs/rust-bitvmx-transaction-monitor
```

OR manually initialize the submodules (if you already cloned the repo without the `--recurse-submodules` option):
 
```bash
$ git clone git@github.com:FairgateLabs/rust-bitvmx-transaction-monitor
$ git submodule init
$ git submodule update --remote --checkout
```

### Setup `.env` File

To run the monitor, you need to create a **.env** file. You can use the **.env.example** file as a reference.

### Bitvmx Instances Data

You can add bitvmx instances to be detected in **bitvmx_instances_example.rs** inside src folder.

### Envs/Args

To check Possible run

```
cargo run -- --help
```

