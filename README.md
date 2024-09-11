# Bitcoin transaction monitor

This process checks all BitVMX instances in a file and verifies them against a Bitcoin indexer to ensure that the transactions associated with the BitVMX instances are found in the blockchain.

### Setup `.env` File

To run the monitor, you need to create a **.env** file. You can use the **.env.example** file as a reference.

### Bitvmx Instances Data

You can use bitvmx_data_example.json as a reference. Copy and paste this file, then rename it. Remember to use the same name for the file as you have declared in your data configuration under BITVMX_FILE_PATH.

### Envs/Args

To check Possible run

```
cargo run -- --help
```

