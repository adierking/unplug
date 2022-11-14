# :electric_plug: Unplug - Chibi-Robo! Modding Toolkit

![Unplug is a Rust library and command-line interface for working with Chibi-Robo! assets.](docs/images/unplug.gif)

[![Release](https://img.shields.io/github/v/release/adierking/unplug)](https://github.com/adierking/unplug/releases)
[![Build Status](https://img.shields.io/github/workflow/status/adierking/unplug/Rust)](https://github.com/adierking/unplug/actions)
[![Test Coverage](https://img.shields.io/coveralls/github/adierking/unplug)](https://coveralls.io/github/adierking/unplug)
[![License](https://img.shields.io/github/license/adierking/unplug)](COPYING)

:satellite: [Download](https://github.com/adierking/unplug/releases)<br>
:star: [Goals](#goals)<br>
:robot: [Features](#features)<br>
:thinking: [How to Use](#how-to-use)<br>
:gear: [Compiling](#compiling)<br>
:wrench: [Contributing](#contributing)<br>

## Download

Go to the [releases page](https://github.com/adierking/unplug/releases) to download a prebuilt
binary for Windows, macOS, or Linux.

You can also get bleeding-edge continuous builds
[here](https://github.com/adierking/unplug/actions), though these are undocumented and not
guaranteed to work correctly.

## Goals

- **Modding**: Build a foundation for editing game assets
- **Reverse Engineering**: Help the community learn more about how the game works
- **Self-Contained**: Unplug should be the only tool you need to mod the game
- **Correctness**: What you see is what the game sees

## Features

- Built-in support for ISO reading and writing
- Audio export, import, and playback
- Edit objects in stage files
- Export and import cutscene messages
- Change items in the shop
- Edit the global metadata
- Disassemble script bytecode

Stay tuned for more!

## How to Use

You will need an NTSC-U (GGTE01) *Chibi-Robo! Plug Into Adventure!* ISO to use Unplug. Other
versions of the game are not supported yet.

Unplug is a command-line app, so you'll need to open PowerShell/Command Prompt/Terminal to use
it. On Windows 10+, you should try downloading Windows Terminal from the Store.

Each function provided by Unplug is a subcommand of the main program. Running Unplug without any
command-line arguments or with `help` will display a list of available commands.

Follow the [tour guide](docs/tour.md) for examples of how to use each command.

## Compiling

Prerequisites:

- Rust 1.65+
- C compiler (for dependencies)
- CMake (for dependencies)

You can compile with Cargo as usual:

```sh
cargo build
cargo run -- arg...
```

To create a distribution build (i.e. an optimized build to release to others), build with only the
`distribution` feature activated. This will strip debugging features from the program:

```sh
cargo build --release --no-default-features --features distribution
```

To build and run the unit tests:

```sh
cargo test --lib
```

To run the full test suite, you will need to point the `CHIBI_ISO` environment variable to a
GGTE01 ISO. In PowerShell, you can do that like this:

```powershell
$Env:CHIBI_ISO="C:\path\to\the.iso"
cargo test
```

The tests will not modify the ISO, but some will copy it to your temporary directory - this means
you will need 1.4 GB of free space.

## Contributing

There are lots of ways you can contribute to the project:

- Submit issue reports
- Open pull requests to fix bugs or add new features
- Help map out unknown structs/opcodes
- Build higher-level tools (e.g. GUIs) on top of Unplug
- Make and share cool mods!

Before implementing a complex feature, you should reach out to Derpky through the
[community Discord](http://discord.gg/ymNDqTyjRQ) to discuss it.

Unplug is currently licensed under the MIT license.
