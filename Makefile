# Tape — build and test. Needs: rustup target add wasm32-unknown-unknown; cargo-near (https://github.com/near/cargo-near).

.PHONY: wasm test unit e2e demo demo-testnet

wasm:            ## build both contracts for NEAR
	cd contracts/usdc-mock && cargo near build non-reproducible-wasm --no-abi
	cd contracts/tape && cargo near build non-reproducible-wasm --no-abi

unit:            ## the rules: venue, guard, calls (plain Rust, no chain)
	cargo test -p tape-venue -p tape-guard -p tape-calls

e2e: wasm        ## the whole story on a local NEAR sandbox (downloads the sandbox node once)
	cd contracts/tape && cargo test --test e2e

test: unit e2e

demo: wasm       ## one recorded NEAR day, narrated, on a local sandbox
	cargo run -q -p tape-demo

demo-testnet:    ## the same story on the live testnet deployment (needs the testnet keys)
	cargo run -q -p tape-demo -- testnet
