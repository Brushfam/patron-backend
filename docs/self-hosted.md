# Self-hosted server

You can utilize your own infrastructure and deploy your own version of a deployment server and frontend.

For deployment/API server, there are four separate components to use:

* API server
* Smart contract builder
* On-chain event listener
* Database migration manager

If necessary, these components can be deployed on different servers with the single database attached to them.

## Configuration

All of these components use the same configuration file `Config.toml`. The example file looks like this:

```toml
[database]
# Database URL (preferrably PostgreSQL).
url = "postgres://user:password@127.0.0.1/db"

[server]
# HTTP server listen address.
address = "127.0.0.1:3000"

[logging]
# Minimal logging level
level = "info"

[builder]
# Path where to store temporary build images
images_path = "/tmp/images"
# The amount of build workers to start simultaneously.
worker_count = 1
# Build duration limit, after which the container if forcefully deleted (in seconds).
max_build_duration = 3600
# Max WASM file size (in bytes).
wasm_size_limit = 5242880
# Max JSON metadata file size (in bytes).
metadata_size_limit = 1048576
# RAM limit for each build session (in bytes).
memory_limit = 8589934592
# RAM + Swap limit for each build session (in bytes, should include memory_limit).
memory_swap_limit = 8589934592
# Max temporary image size for each build session.
volume_size = "8G"

[storage]
# S3 access key id.
access_key_id = "..."
# S3 secret access key.
secret_access_key = "..."
# S3 region.
region = "us-east4"
# S3 endpoint URL.
endpoint_url = "..."
# S3 bucket name to store source code archives.
source_code_bucket = "test-bucket"
```

You can also pass configuration values using `CONFIG_` environment variables.

For example, to set the server address, you can use the `CONFIG_SERVER_ADDRESS` environment variable.
Setting more complex values requires using a structural syntax. For example, setting storage config using
environment variables would look like this:

```sh
CONFIG_STORAGE="{\
    access_key_id=...,\
    secret_access_key=...,\
    region=...,\
    endpoint_url=...}\
"
```

## Installation

For installation of different server components it's recommended to utilize our Nix environment, which
allows you to build Docker images and binaries with ease.

Building all components:

```sh
nix build .#
```

You can also use a standard Rust toolchain for this task:

```sh
cargo build --release
```

To build the Docker image from scratch, you can utilize the next command:

```sh
nix build .#ink-builder
```

Pre-built Docker images are available to download from GitHub releases.

## API server

API server is required to handle client requests and generally has to be available to a user network.

To start an API server, simply run its binary with the configured `Config.toml`:

```sh
./server
```

Be sure to install a separate proxy server to handle TLS termination and resource limiting.

## Smart contract builder

To deploy the smart contract builder, there are several prerequisites required:

* Docker to use for the build process itself;
* `fallocate` and `mkfs.ext4` commands to create new temporary volumes;
* udisks2 with loop device setup capabilities to allow mounting temporary volumes;

The build process is done using the Docker `ink-builder` image, so ensure that you have
it loaded using the `docker load` command.

To start the builder process you can use the `serve` command:

```sh
./builder serve
```

## On-chain event listener

This component provides users with the information about events on-chain.

To initially fill the database with contract and code data use the `initialize` command:

```sh
./event_client initialize my_node wss://node.example.com:443/ astar
```

`initialize` command accepts the node name, node URL, and node schema, which is used to
correctly communicate with the node.

You may also optionally pass `--payment-address` flag to enable membership payments using a separate smart contract.

To fill the database with missing events you can use the `traverse` command, which traverses old blocks
for previous chain events:

```sh
./event_client traverse my_node
```

Watching for new chain events is available with the `watch` command:

```sh
./event_client watch my_node
```

Event watcher will also attempt to traverse any missed blocks automatically.

For more information about available commands use the `--help` flag.

## Database migration manager

Using the migration manager you can fill the database with the necessary for service functionality tables.

To fill the database, simply run the migration manager binary with the configured `Config.toml`:

```sh
./migration
```
