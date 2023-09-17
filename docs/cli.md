# CLI

Using our CLI, you can authenticate and deploy your smart contracts in an instant, with a vastly simplified deploy flow.

For deploy purposes, ensure that you have the Rust toolchain installed (the builds themselves are not local, but `cargo` is used to install and invoke `cargo-contract`).

## Authentication

To authenticate, use the `auth` subcommand, which automatically redirects you to website to sign an authentication message:

```sh
patron auth
```

If you are using a custom server, you can also pass `-s` and `-w` flags to provide URLs for the API server and website.

```sh
patron auth -s https://api.example.com -w https://example.com
```

Custom server URLs are later propagated to other commands (such as deploy) automatically.

## Deploy

The build process itself is done on a remote server, but the deployment process is done locally to keep your private keys
safe and to facilitate possible air-gapped deployments.

First of all, you need to create a `Deploy.toml` file at the root of your contract source code.
This file describes the `cargo-contract` version that will be used during the build:

```toml
cargo_contract_version = "3.1.0"
```

You can check this file into your VCS to share the same configuration with your development team.

To start the deploy process for locally running development node simply pass the constructor name and secret URI for the private key:

```sh
patron deploy new --suri //Alice
```

If your contract constructor requires any arguments, simply pass them with the same syntax that you use with the `cargo-contract`:

```sh
patron deploy new --args 123 --suri //Alice
```

Custom node URL can be provided with the `--url` flag:

```sh
patron deploy new --url wss://node.example.com:443 --suri ...
```

You can also pass arbitrary flags to `cargo-contract` using `--` syntax:

```sh
patron deploy new --suri //Alice -- --password 123
```

To get more information, invoke the deploy command with the `--help` flag.

## Build

You can also acquire contract's WASM blob and JSON metadata files without the deployment itself
by using the `build` subcommand which, by default, outputs `contract.wasm` and `contract.json` files
to the `./target/ink` directory.

You can modify the output directory with `--wasm_path` and `--metadata_path` flags.

See `--help` flag output for more information.

## Watch

File watch functionality allows you to simplify your build-deploy-interact cycle during the development process
with an automatically refreshed contract caller and contract builder invoked on any meaningful file change.

To start watching, provide the constructor name and `suri` to the `watch` subcommand:

```sh
patron watch new --suri //Alice
```

You can use almost any flag available in the [`deploy` subcommand](#deploy).

File watcher will automatically deploy your contract using the provided configuration, so ensure that
constructor ABI is the same between each re-build.

