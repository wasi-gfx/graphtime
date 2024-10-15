### Temporary wasm runtime with wasi and wasi-webgpu support
This runtime will be deprecated once more popular runtimes add wasi-webgpu support.


#### Usage:
```shell
cargo run [path to wasm component]
```
### Building components that run in graphtime

Please note that graphtime requires guest components to export the [`wasi:cli/run` interface](https://github.com/WebAssembly/WASI/blob/8777909de3d1cb5cdaee16f98d76c572d4b88b16/wasip2/cli/run.wit#L2).
