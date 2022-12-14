# wasm-chip8
Chip8 emulator compiled to WebAssembly

# Prequisites

- Cargo
- Node
- Wasm-pack

## Wasm-pack

Wasm-pack is the tool automating WebAssembly packaging provided by rustwasm team. It can be easily installed using following command:
```
cargo install wasm-pack
``` 

# Building

In the root of the project
```
wasm-pack build
```

Then in the www directory
```
npm install
npm run start
```

The app should be running in `localhost:8080`