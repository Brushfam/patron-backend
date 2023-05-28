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
This file describes the Rust toolchain and `cargo-contract` versions that will be used during the build:

```toml
rustc_version = "1.69.0"
cargo_contract_version = "3.0.1"
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
