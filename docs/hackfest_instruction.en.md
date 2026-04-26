> 🇩🇪 [Auf Deutsch lesen](hackfest_anleitung.de.md) | 🇬🇧 **English**

# Setup Guide: CDA and SOVD on the Raspberry Pi

## Table of Contents
- [Introduction](#introduction)
  - [Repositories](#repositories)
  - [Packages](#packages)
- [Setup via Starter Script](#setup-via-starter-script)
- [Preparing the CDA](#preparing-the-cda)
- [Preparing the SOVD Server](#preparing-the-sovd-server)
- [Flashing the S32K148 Board](#flashing-the-s32k148-board)

## Introduction

The following guide contains the steps to create a test CDA-SOVD setup. The procedure was tested with `Raspberry Pi OS lite (Debian GNU/Linux 13.04 "Trixie", 64-bit)` on a Raspberry Pi 5 with 8 GB RAM.
The CDA is simulated using the test container available in the `classic-diagnostic-adapter` repository linked below. For more information, see the README.md in `classic-diagnostic-adapter/testcontainer/README.md`

### Repositories
Required repositories for this setup are:
- HSE-DNS/opensovd-server, Branch sovd-connection-cda: https://github.com/HSE-DNS/opensovd-server/tree/sovd-connection-cda
- eclipse-opensovd/classic-diagnostic-adapter: https://github.com/eclipse-opensovd/classic-diagnostic-adapter/

### Packages

The additionally required packages can be installed with the following commands:
- Docker:
```sh
curl -fsSl https://get.docker.com | sh
```
```sh
sudo usermod -aG docker $USER
```
Note: Afterwards, please log out and log back in for the changes to take effect.
- Rust:
```sh 
curl https://sh.rustup.rs -sSf | sh
```
```sh
source $HOME/.cargo/env
```

- jq:
```sh
sudo apt install jq
```

The setup was tested with the following versions:

| Package    | Version        |
|------------|----------------|
| Docker     | 29.4.1         |
| Rust       | 1.94.0-nightly |
| jq         | 1.7            |


## Setup via Starter Script
If the CDA test containers are already built, the [starter.sh](../starter.sh) script from this repository can be used.
ATTENTION: Under certain circumstances, the script may need to be made executable with the command: `chmod +x starter.sh`
 
The script can be started with the following parameters:

- `start`: starts the CDA test containers, retrieves an access token, and boots up the SOVD server with it
- `stop`: stops the SOVD server and shuts down the CDA test containers
- `status`: shows the status of the CDA test containers

## Preparing the CDA

Info: A detailed description of the CDA and the CDA provider can be found in the [docs/](../docs/) directory.

In order for the test container to run on the Raspberry Pi, the corresponding containers must be built for arm64. This can be done either directly on the Raspberry Pi or on x86 devices via emulation.
Further information can be found in the README of the CDA repository.

a) ARM devices (MAC or Pi):

Starting from the root directory of the CDA repository, the images are built as follows:
```sh
cd testcontainer && docker compose build
```

b) x86 devices

For cross-compiling on x86, the QEMU emulators may have to be installed additionally. This is done with the command
```sh
docker run --privileged --rm tonistiigi/binfmt --install all
```
Additional information on this: https://docs.docker.com/build/building/multi-platform/#install-qemu-manually

Afterwards, the build can be started in the `testcontainer` directory with
```sh
DOCKER_DEFAULT_PLATFORM=linux/arm64 docker compose build
```
Note: The `docker buildx build` command used in the Docker documentation linked above can only be used to build a **single** image, but **not** for a `docker compose` command.

If PowerShell is used instead of a Linux environment, the command changes to
```sh
$env:DOCKER_DEFAULT_PLATFORM="linux/arm64"; docker compose build
```

After building, the images must be transferred to the Pi. This can be done, for example, by:
```sh
docker save -o pi-testcontainer.tar testcontainer-ecu-sim-arm64:latest testcontainer-cda:latest # creates an archive with the images
```
```sh
scp pi-testcontainer.tar <username>@<Pi_IP>:/home/<username>/workspace/ # or any other directory
```

On the Pi itself, these images must then be imported, assumed here to be in the `workspace/` directory:
```sh
cd /home/<username>/workspace/
docker load -i pi-testcontainer.tar
```

Note: If the provided or built images do not have the default names `testcontainer-ecu-sim:latest` and `testcontainer-cda:latest`, the image name must be specified for the respective job in the `docker-compose.yml` of the CDA `testcontainer` directory, otherwise the images will not be found and consequently rebuilt, which may take a long time. 

The following example assumes that the images are named `ecu-sim-arm64:latest` and `cda-arm64:latest`. Since they do not match the default naming, 
```yaml
services:
  ecu-sim:
    build:
      context: ./ecu-sim
      dockerfile: docker/Dockerfile
	(...)
  cda:
    build:
      context: ..
      dockerfile: testcontainer/cda/Dockerfile
```
must be changed to:
```yaml
services:
  ecu-sim:
    image: ecu-sim-arm64:latest # NEW
    build:
      context: ./ecu-sim
      dockerfile: docker/Dockerfile
	(...)
  cda:
    image: cda-arm64:latest # NEW
    build:
      context: ..
      dockerfile: testcontainer/cda/Dockerfile
```

Once the images are built or loaded, the network can be started in the `testcontainer` directory of the CDA repository with the command
```sh
docker compose up -d
```
To shut down, use the command
```sh
docker compose down
```

## Preparing the SOVD Server
If the starter.sh script is not used, the SOVD server can also be built manually. This can be done using the command
```sh
cargo build -p opensovd-gateway
```
On the Pi, if there are not enough resources available, it can optionally be built with the `--release` flag and/or the number of used cores can be limited with `-j 2`.

Note: If the server is built with the `--release` flag and you still want to use the starter script, the path in line 34 of the `starter.sh` script must be changed to `./target/release/opensovd-gateway`. 

The server can be started either generally:
```sh
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```
or in conjunction with an access token from the CDA:
```sh
CDA_TOKEN="the_cda_access_token" cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

When starting the SOVD server, parameters that deviate from the default values can be passed as flags:
- `--cda-host`
- `--cda-port`
- `--cda-base-path`
- `--cda-token`
- `--url`

or, as seen above for example with the `CDA_TOKEN`, via an environment variable. 

Further information can be found, as already mentioned, in [cda.md](cda.md) and [cdaProvider.md](cdaProvider.md). 

## Flashing the S32K148 Board
For flashing the S32K148 board, please use the provided documentary: https://eclipse-openbsw.github.io/openbsw/sphinx_docs/doc/dev/learning/setup/index.html

NOTE: 
- The documentation references Ubuntu 22.04, however the instructions were also tested on Ubuntu 24.04 LTS without any problems
- The documentation mentions the use of 
    ```sh
    cmake --preset s32k148-gcc
    cmake --build --preset s32k148-gcc
    ```
    for CMake target builds. However, this target does no longer exist.
    Instead, for the boards used at the SDV HackFest, the correct target/commands should be:
    ```sh
    cmake --preset s32k148-freertos-gcc
    ```
    ```sh
    cmake --build --preset s32k148-freertos-gcc
    ```
