# A Tour of Unplug 0.4

Unplug 0.4 is a pretty big change from 0.3. There is a new "project" paradigm for managing ISOs and
all of the commands have been reworked. This guide gives you an overview of all of Unplug's major
features so that you can get up to speed quickly.

## Table of Contents

- [Projects](#projects)
- [Running Dolphin](#running-dolphin)
- [Editing Stages](#editing-stages)
- [Editing Audio](#editing-audio)
- [Editing Cutscene Messages](#editing-cutscene-messages)
- [Editing the In-Game Shop](#editing-the-in-game-shop)
- [Editing Global Metadata](#editing-global-metadata)
- [Dumping Scripts](#dumping-scripts)
- [Working with the ISO](#working-with-the-iso)
- [Working with qp.bin](#working-with-qpbin)

## Projects

In older versions of Unplug, you had to pass `--iso` to every command, which got tedious very
quickly. Unplug 0.4 introduces a "project" system to improve upon this. Essentially, you can
register your ISO files as "projects" and then "open" a project to automatically use it for future
commands.

To get started with the project system, you'll want to set the "default ISO". This should point to
an unmodified ISO of the game:

```sh
$ unplug config set default-iso 'E:\Dolphin\chibi.iso'
```

This will already help you out a lot - now, even if you don't have a project open, commands will
automatically use that default ISO! And don't worry - as a safety mechanism to keep the file clean,
you will never be allowed to edit this file, only read from it.

To test that this worked, try running the `iso info` command:

```sh
$ unplug iso info
```

Now let's say we want to start working on a new copy of the ISO called `telly_is_a_toy.iso`. You
could manually go to the folder where it's stored, copy it, and rename the copy, but you could also
just use the `project new` command:

```sh
$ unplug project new telly_is_a_toy
$ unplug project open telly_is_a_toy
```

This will make a copy of the default ISO and switch to it for future commands. Now you can use any
editing commands you want and they'll all go to this ISO.

You can close the project and go back to the default ISO by using `project close`, and you can pass
`--default-iso` to any command to temporarily switch back.

## Running Dolphin

Unplug 0.4 introduces a `dolphin` command which lets you quickly run the Dolphin Emulator with your
current project.

If this is your first time using Unplug 0.4, you'll need to configure where Dolphin is installed:

```sh
$ unplug config set dolphin-path 'D:\Downloads\Dolphin-x64\Dolphin.exe'
```

On macOS, you can also point this to Dolphin.app:

```sh
$ unplug config set dolphin-path ~/Applications/Dolphin.app
```

Now you just need to run the `dolphin` command any time you want to test your project!

```sh
$ unplug dolphin
```

## Editing Stages

Unplug 0.4 introduces a `stage` command which gives you the ability to export stage data to .json
files which you can edit and then re-import.

To export the stage data, use the `stage export-all` command:

```sh
$ unplug stage export-all -o stages
```

This will make a directory named `stages` which contains all of the .json files for each stage. For
details on the format of these files, see the [stage JSON format reference](/docs/stage.md).

After you've edited any stage files, you can reimport them with `stage import-all`. This will scan
the `stages` folder for any stages you edited and rebuild them.

```sh
$ unplug stage import-all stages
```

To go back to the default stage data, use `stage export-all` with `--default-iso`:

```sh
$ unplug --default-iso stage export-all -o stages
```

## Editing Audio

Unplug 0.4 introduces an `audio` command which can export, import, and play the game's audio
resources.

To play a sound file, use the `audio play` command:

```sh
$ unplug audio play bgm
$ unplug audio play voice_tonpy_1
```

To export all the sound files in WAV format, use the `audio export-all` command (warning, this is large!):

```sh
$ unplug audio export-all -o out/audio
```

To replace a sound file, use the `audio import` command with any WAV, FLAC, MP3, or OGG file:

```sh
$ unplug audio replace bgm "Two Trucks.mp3"
$ unplug audio replace voice_tonpy_1 boog.wav
```

A tutorial on more-advanced sound editing is coming soon.

## Editing Cutscene Messages

The `messages` commands let you export cutscene messages to an XML file which you can edit and
then re-import. This rebuilds the game's data files, so it isn't subject to the usual limitations
of hex editing.

To export the messages from your ISO, use the `messages export` command:

```sh
$ unplug messages export -o messages.xml
```

This will make a file named `messages.xml` which you can edit. To re-import the messages, just make
sure a project is open and then use the `messages import` command:

```sh
$ unplug messages import messages.xml
```

## Editing the In-Game Shop

The `shop` commands let you edit the in-game shop and change what items are available.

To export the shop rules from your ISO, use the `shop export` command:

```sh
$ unplug shop export -o shop.json
```

This will make a file named `shop.json` which you can edit. You can edit each slot's
item, price, limit, and requirements. (Note that there is a hard limit of 20 items.) Item IDs are
mostly just lowercased and snake_cased versions of the in-game names, but if you
 need to check which names are available then you can use the `list items` command:

```sh
$ unplug list items
```

After you're done editing the shop, make sure a project is open and then use the `shop import`
command:

```sh
$ unplug shop import shop.json
```

## Editing Global Metadata

The global metadata includes item attributes, room names, attachment settings, battery usage
settings, and more.

To export the globals from your ISO, use the `globals export` command:

```sh
$ unplug globals export -o globals.json
```

This will make a file named `globals.json` which you can edit. To re-import the globals, make sure a
project is open and then use the `globals import` command:

```sh
$ unplug globals import globals.json
```

## Dumping Scripts

Most of the game's logic is powered by a custom scripting engine with bytecode stored in each
stage file. Unplug doesn't have a language which lets you edit this yet, but you can at least dump
its internal representation of the script data. Use the `script dump-all` command to do that:

```sh
$ unplug script dump-all -o scripts
```

Note that this internal representation is subject to change at any time, so try to avoid parsing it
in another program.

## Working with the ISO

If you want to manually view/edit raw game files, the first thing you'll probably want to do is
extract the game ISO. You can use the `iso extract-all` command to do this:

```sh
$ unplug iso extract-all -o iso
```

This will make a directory named `iso` which contains the contents of the ISO.

If you only want to export specific files, you can also use the `iso extract` command and then list
the files you want. It even supports globbing, so you could do this to extract all the music files:

```sh
$ unplug iso extract -o music '**.hps'
```

And finally, if you want to re-import a file after you've edited it yourself, you can use the `iso
replace` command:

```sh
$ unplug iso replace qp/streaming/bgm.hps my_bgm.hps
```

## Working with qp.bin

To further dive into the game files, you'll also need to extract `qp.bin`, which is an archive
file inside the ISO. Use the `qp` command to do this:

```sh
$ unplug qp extract-all -o out/qp
```

This will make a directory named `qp` which contains the contents of qp.bin.

The `qp` command works almost exactly like the `iso` command, so you can extract and replace
specific files as well:

```sh
$ unplug qp extract -o stages 'bin/e/*.bin'
$ unplug qp replace bin/e/stage07.bin my_stage07.bin
```