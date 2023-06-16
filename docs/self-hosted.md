# Self-hosted server

You can utilize your own infrastructure and deploy your own version of a deployment server and frontend.

For deployment/API server, there are four separate components to use:

* API server
* Smart contract builder
* On-chain event listener
* Database migration manager

If necessary, these components can be deployed on different servers with the single database attached to them.

## Repository cloning

First, clone the repository using the following command:

```sh
git clone https://github.com/brushfam/patron-backend
```

## Nix installation

We use the [Nix package manager](https://nixos.org) to build Docker images and various project components.

Linux installation instructions are available [here](https://nixos.org/download.html#nix-install-linux).

Since we utilize Nix flakes you have to configure Nix CLI to support them.

1. Create a `~/.config/nix` directory using the following command: `mkdir -p ~/.config/nix`
2. Enable `flakes` and `nix-command` features via the following command: `echo "experimental-features = nix-command flakes" > ~/.config/nix/nix.conf`

## PostgreSQL

PostgreSQL installation instruction may vary between different Linux distributions, the following guide is suitable for Ubuntu.

1. Install PostgreSQL via the following command: `sudo apt install postgresql postgresql-contrib`
2. Start PostgreSQL using this command: `sudo systemctl start postgresql.service`
3. Log in as a `postgres` user: `sudo su postgres`
4. Create a new PostgreSQL user that can create new databases and that has a password: `createuser -dP <name>`.
Replace `<name>` with a name that suits your project.
5. Create a new database: `createdb -h 127.0.0.1 -U <name> <database>`. Replace `<name>` with the user name
you specified in the previous step, and `<database>` with your preferred database name.

## Docker

Follow the [official guide](https://docs.docker.com/engine/install/) on how to install Docker on various platforms.

Don't forget to add your system user to the `docker` group to run Docker commands without root: `sudo usermod -a -G docker <user>`

## Configuration

All of these components use the same configuration file `Config.toml`. The example file looks like this:

```toml
[database]
# Database URL (preferrably PostgreSQL).
url = "postgres://<name>:<password>@127.0.0.1/<database>"

[server]
# HTTP server listen address.
address = "127.0.0.1:3000"

[logging]
# Minimal logging level
level = "info"

[builder]
# Path where to store temporary build images
images_path = "/tmp/images"
# URL of an API server
api_server_url = "https://api.example.com"
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
    endpoint_url=...,\
    source_code_bucket=...}\
"
```

## Installation

Building all components with Nix (this will automatically install all the necessary tools):

```sh
nix build .#
```

You can also use a standard Rust toolchain for this task:

```sh
cargo build --release
```

## Smart contract builder image

To build the Docker image itself, you can utilize the next command:

```sh
nix build .#docker.ink-builder
```

Load the resulting image with this command:

```sh
docker load < result
```

## Database migration manager

Using the migration manager you can fill the database with the necessary for service functionality tables.

To fill the database, simply run the migration manager binary with the configured `Config.toml`:

```sh
./migration
```

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
* udisks2 with loop device setup capabilities to allow mounting temporary volumes (can be installed via `sudo apt install udisks2`);

You can test your Linux distribution readiness for the requirements above using the following commands
(all of these commands must work without a root user interaction):

```sh
fallocate -l 50M image
mkfs.ext4 image
udisksctl loop-setup --no-user-interaction -f image
```

If loop device setup is successful without, you can remove the test file and proceed with the next part
of this guide:

```sh
udisksctl loop-delete --no-user-interaction -b /dev/loop8
rm image
```

If you have any errors related to `udisksctl loop-setup` permissions,
see the ["Troubleshooting"](#troubleshooting) section of this guide.

Ensure that you completed the process of building the builder image and loading it in
the previous ["Smart contract builder image"](#smart-contract-builder-image) section of this guide.

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
See the ["Membership smart contract ABI"](#membership-smart-contract-abi) for more information on that.

Watching for new chain events is available with the `watch` command:

```sh
./event_client watch my_node
```

Event watcher will also attempt to traverse any missed blocks automatically.

For more information about available commands use the `--help` flag.

## Troubleshooting

### `udisksctl loop-setup` permissions

`udisks2` daemon utilizes PolicyKit to manage user permissions to
invoke various `udisksctl` commands.

If you manage your system permissions via `polkit`, ensure
that your user has an access to invoke the `org.freedesktop.udisks2.loop-setup` action.

Example `polkit` rule, that allows all users in the `sudo` group to setup loop devices:

```javascript
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.udisks2.loop-setup" && subject.isInGroup("sudo")) {
        return polkit.Result.YES;
    }
});
```

JavaScript-based rules are placed inside of a following directory: `/etc/polkit-1/rules.d`.
For example: `/etc/polkit-1/rules.d/50-loop.rules`.

Example PolicyKit rule, if your distribution uses it to manage system permissions:

```
[Storage Permissions]
Identity=unix-group:sudo
Action=org.freedesktop.udisks2.loop-setup
ResultAny=yes
```

PolicyKit rules can be placed inside of a following directory: `/etc/polkit-1/localauthority`.
For example: `/etc/polkit-1/localauthority/50-local.d/55-loop.pkla`.

## Membership smart contract ABI

You can use any smart contract you want, as long as it adheres to the following ABI schema:

* Your contract provides a method with an identifier equal to the `Blake2b256` hash value of a string "check".
* This method accepts a single argument which is the address of an account that is being checked.
* This method returns a single `bool` value which identifies if the check was successful or not.
