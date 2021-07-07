# fishnet: distributed Stockfish analysis for lichess.org

[![crates.io](https://img.shields.io/crates/v/fishnet.svg)](https://crates.io/crates/fishnet)
[![Build](https://github.com/niklasf/fishnet/workflows/Build/badge.svg)](https://github.com/niklasf/fishnet/actions?query=workflow%3ABuild)
[![Docker](https://img.shields.io/docker/cloud/build/niklasf/fishnet)](https://hub.docker.com/r/niklasf/fishnet)

## Installation

1. Request your personal fishnet key: https://lichess.org/get-fishnet

2. Install and run the fishnet client.

   **Download standalone binary**

   Select the binary for your platform
   [from the latest release](https://github.com/niklasf/fishnet/releases)
   and run it.

   ```sh
   # After download:
   chmod +x fishnet-x86_64-unknown-linux-gnu
   ./fishnet-x86_64-unknown-linux-gnu --auto-update
   ```

   Other useful commands:

   ```sh
   ./fishnet-x86_64-unknown-linux-gnu configure              # Rerun config dialog
   ./fishnet-x86_64-unknown-linux-gnu systemd --auto-update  # Print a .service file
   ./fishnet-x86_64-unknown-linux-gnu --help                 # List commands and options
   ```

   **From source**

   Assuming you have [a recent Rust toolchain](https://rustup.rs/), a C++ compiler, strip, and make installed:

   ```sh
   git clone --recursive https://github.com/niklasf/fishnet.git
   cd fishnet
   git submodule update
   cargo run --release -vv --
   ```

   **Docker**

   ```sh
   docker run -it --name fishnet -e KEY=abcdef niklasf/fishnet:2
   ```
    **Kubernetes**  
    Docker desktop? You can enable support for kubernetes and run inside it!  
      
    Edit fishnet.yaml file, Search for property named: <code>'fishnet-private-key'</code>.  
    Replace the value with your actual <i>fishnet private key</i> encoded with BASE64. 
      
    Sample:  
    ```sh
    
    Plain text:  
    KEY=abcdef
    Base64 encoded text  
    KEY=YWJjZGVm
    ```
3. Pick an update strategy.

   **Automatic updates**

   Run with `--auto-update` as recommended above (will currently still require
   manual restarts on Windows
   due to [#151](https://github.com/niklasf/fishnet/issues/151)).

   **Subscribe to release announcements**

   With a GitHub account, you can watch this repository (can be set to only
   release announcements). See the top right corner on this page.
   
   **Docker**
   
   The docker container was named fishnet, so we can update the image and restart it
   
   ```sh
   docker rm fishnet
   docker pull niklasf/fishnet:2
   docker run -it --name fishnet -e KEY=abcdef niklasf/fishnet:2
   ```
   **Kubernetes**  
   Pod has imagepull policy set to Always, deleting the pod and recreating will update to latest version.  
   ```sh
   kubectl delete pod fishnet-pod -n=fishnet
   kubectl apply -f fishnet.yaml
   ```
   You can always update the fishnet.yaml file to use specific version of image.  

## Video introduction

Watch [@arex](https://lichess.org/@/arex) explain fishnet.

[![Video introduction](https://img.youtube.com/vi/C2SjcVbRfp0/0.jpg)](https://youtu.be/C2SjcVbRfp0)

## FAQ

### Which engine does fishnet use?

fishnet uses [Stockfish](https://github.com/official-stockfish/Stockfish)
(hence the name) and [Fairy-Stockfish](https://github.com/ianfab/Fairy-Stockfish)
for chess variants.

### What are the requirements?

| Available for | 64-bit Intel and AMD        | ARMv8 / Silicon             |
| ------------- | --------------------------- | --------------------------- |
| **Linux**     | `x86_64-unknown-linux-gnu`  | build from source           |
| **Windows**   | `x86_64-pc-windows-gnu.exe` |                             |
| **macOS**     | `x86_64-apple-darwin`       | `aarch64-apple-darwin`      |
| **FreeBSD**   | build from source           |                             |

- Needs an operating system from around 2016 or later
- Will max out the configured number of CPU cores
- Uses about 64 MiB RAM per CPU core
- A small amount of disk space
- Low-bandwidth network communication with Lichess servers
  (only outgoing HTTP requests, so probably no firewall configuration
  required)

### Is my CPU fast enough?

Almost all processors will be able to meet the requirement of ~2 meganodes in
6 seconds. Clients on the faster end will automatically be assigned
analysis jobs that have humans waiting for the result (the user queue, as
opposed to the system queue for slower clients).

### What happens if I stop my client?

Feel free to turn your client on and off at any time. By default, the client
will try to finish any batches it has already started. On immediate shutdown,
the client tries to inform Lichess that batches should be reassigned.
If even that fails, Lichess will reassign the batches after a timeout.

### Will fishnet use my GPU?

No, Stockfish is a classical alpha-beta engine. The neural network evaluation
of Stockfish NNUE works efficiently on CPUs.

### Is fishnet secure?

To the best of our knowledge. However you implicitly trust the authors and the
GitHub infrastructure when running with `--auto-update`.

You can mitigate this by running fishnet as an unprivileged user.

[`cargo-crev`](https://github.com/crev-dev/cargo-crev) is used to review the
trustworthiness of dependencies.

### Is there a leaderboard of contributors?

No, sorry, not publically. It would incentivize gaming the metrics.

### Can I autoscale fishnet in the cloud?

There is currently no ready-made solution, but
[an API for monitoring the job queue status](https://github.com/niklasf/fishnet/blob/master/doc/protocol.md#status)
is provided.

## Protocol

![Sequence diagram](https://raw.githubusercontent.com/niklasf/fishnet/master/doc/sequence-diagram.png)

See [protocol.md](https://github.com/niklasf/fishnet/blob/master/doc/protocol.md) for details.
Also supports [`SSLKEYLOGFILE`](https://developer.mozilla.org/en-US/docs/Mozilla/Projects/NSS/Key_Log_Format) for inspection at runtime.

## License

fishnet is licensed under the GPLv3+. See LICENSE.txt for the full
license text.
