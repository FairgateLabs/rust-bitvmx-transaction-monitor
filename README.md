# Bitvmx Instance Monitor

This process checks BitVMX instances and verifies them against a Bitcoin indexer to ensure that each transactions associated with the BitVMX instance are found in the blockchain.

## Installation
Clone the repository and initialize the submodules:
```bash
$ git clone git@github.com:FairgateLabs/rust-bitvmx-transaction-monitor
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

