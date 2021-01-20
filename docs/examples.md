# Examples

## Editing Cutscene Messages

Unplug provides the ability to export cutscene messages to an XML file which you can edit and
then re-import. This rebuilds the game's data files, so it isn't subject to the usual limitations
of hex editing.

To export the messages from your ISO, use the `export-messages` command:

```sh
unplug export-messages --iso chibi.iso -o messages.xml
```

This will make a file named `messages.xml` which you can edit. To re-import the messages, **make
a copy of your ISO** and then use the `import-messages` command on the copy:

```sh
cp chibi.iso chibi2.iso
unplug import-messages --iso chibi2.iso messages.xml
```

You only need to copy the ISO once; any additional changes can be re-imported into the copy. The
main reason for having a copy is so that you don't trash your retail ISO.

## Dumping Scripts

Most of the game's logic is powered by a custom scripting engine with bytecode stored in each
stage file. To dump all of this, use the `dump-all-stages` command:

```sh
unplug dump-all-stages --iso chibi.iso -o stages
```

This will make a directory named `stages` which contains low-level dumps of the stage files. This
makes use of Rust's `Debug` functionality to keep the implementation simple for now, so the
format is not stable and there is no way to edit any of the files.

## Extracting the ISO

If you want to manually inspect game files, the first thing you'll probably want to do is extract
the game ISO. Use the `extract-iso` command to do this:

```sh
unplug extract-iso chibi.iso -o iso
```

This will make a directory named `iso` which contains the contents of the ISO.

## Extracting qp.bin

To further dive into the game files, you'll also need to extract `qp.bin`, which is an archive
file inside the ISO. Use the `extract-archive` command to do this:

```sh
unplug extract-archive iso/qp.bin -o qp
```

This will make a directory named `qp` which contains the contents of qp.bin.