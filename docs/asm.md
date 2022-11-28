# Unplug Assembly Language

Development builds of Unplug provide the ability to disassemble *Chibi-Robo!* event scripts into an
assembly language syntax and then reassemble them back into bytecode. This is a powerful feature
which gives you full control over the game's scripting engine for cutscenes and object interactions.

This document aims to provide an overview of how to write your own scripts. It is not meant to be
thorough and you should reference the existing scripts in the game to learn more about how specific
things work.

**Note that this feature is still early in development. Error reporting is bad, code is difficult
to read, there may be bugs, and future changes to Unplug may completely break your scripts. This
might not be a great experience yet.**

## Table of Contents

- [Rationale](#rationale)
- [Getting Started](#getting-started)
- [Disassembling Scripts](#disassembling-scripts)
- [Assembling Scripts](#assembling-scripts)
- [Source Structure](#source-structure)
- [Directives](#directives-1)
- [Commands](#commands-1)
- [Operands](#operands)
- [Expressions](#expressions)
- [Message Commands](#message-commands)

## Rationale

The goal behind the assembly language feature is to provide a means of making script data readable
and editable at a low-level. For the most part, each instruction corresponds 1:1 with bytes in the
stage file. While this makes scripts verbose, it has some important benefits:

1. It is easy to exactly understand the structure of script data.
2. Correctness is easy to evaluate by comparing disassembly output.
3. The assembler implementation is relatively simple compared to a high-level language compiler.
4. High-level languages can be built on top of this as a solid foundation. (No promises though!)

Obviously, this language is not the same as the one that the developers used to originally make the
game, as we have no means of knowing what that language actually looked like. Interestingly, though,
whatever language the developers used appears to be rather low-level as well - integers have varying
sizes for seemingly no reason, and there is human error in one of the stage files which resulted in
invalid bytecode that Unplug has to paper over. (In fact, Unplug is designed so that it cannot
produce invalid bytecode, so it's probably more robust than the official compiler in some ways!)

## Getting Started

You'll first want to grab a development build of Unplug from the
[GitHub Actions page](https://github.com/adierking/unplug/actions). Usually you should just be able
to grab the latest build with a green checkmark next to it. Scroll down to the Artifacts window on a
build's page and download the zip for your platform of choice. This guide assumes that you already
know how to configure and use Unplug - if not, read through the [Tour Guide](/docs/tour.md).

It is recommended that you use [Visual Studio Code](https://code.visualstudio.com/) to edit script
files - there's an Unplug extension for auto-formatting and syntax highlighting, and it's also a
great editor to have in general. Once VSCode is installed, run it once and then close it.  Download
the `unplug-vscode.zip` artifact, then go to your user directory -> `.vscode` -> `extensions` and
unzip it into a new folder named `unplug-vscode` to install it. The installed extension should look
something like this:

![Screenshot of the unpacked extension](https://dierking.me/i/i4O2vc.png)

Go to the extensions tab in VSCode to double-check that the Unplug extension is now registered:

![Unplug assembly support extension](https://dierking.me/i/PS0EKj.png)

## Disassembling Scripts

In order to edit anything, you first need to disassemble the game scripts.
Use the `script disassemble-all` command to do this:

```sh
$ unplug --default-iso script disassemble-all -o scripts
```

This will make a "scripts" directory with several `.us` (Unplug Source) files in it. These are the
assembly source files for all the script code in the game.

## Assembling Scripts

Once you've edited a script, all you need to do to see it in-game is to use the `script assemble`
command:

```sh
$ unplug script assemble stage07.us
```

Make sure to open a new project first if you haven't:

```sh
$ unplug project new asm
$ unplug project open asm
```

You can use the `dolphin` command to test your changes if you've configured it:

```sh
$ unplug dolphin
```

## Source Structure

Assembly source files are composed of three main elements: *labels*, *commands*, and *directives*.

### Labels

A *label* associates a bytecode location with a name that can be referenced elsewhere. To define a
label, simply type the label's name followed by a colon:

```
my_cool_label:
```

To use the label elsewhere in the code, type its name prefixed with an asterisk:

```
goto    *my_cool_label
```

### Commands

A *command* is an instruction for the script interpreter. These range from control-flow operations
to manipulating objects and showing cutscenes. Each command must be on its own line, and consists
of an *opcode* followed by a comma-separated list of *operands*:

```
pushbp
setsp   mul(var(0), var(1))
setsp   var(1)
setsp   var(0)
msg     voice(0), format("%d"), " * ", format("%d"), " = ", format("%d"), "!", wait(254)
popbp
return
```

### Directives

A *directive* is an instruction for the assembler - that is, it is interpreted at compile-time and
often does not correspond to bytecode. These allow you to embed raw data into a script and tell the
assembler where entry points are. Their syntax is similar to commands, but they start with a period:

```
        .stage  "stage07"


loc_3246:
        .db     "cb_robo_4.dat"


        .prologue  *evt_prologue
evt_prologue:
```

Note that every script must contain either a `.stage` or `.globals` directive - this is called a
"target specifier" and informs the assembler what to do with the compiled bytecode.

You can also write comments using `;`. These are ignored by the assembler:

```
; Telly is a calculator!
setsp   mul(var(0), var(1))  ; a * b
setsp   var(1)  ; b
setsp   var(0)  ; a
```

## Directives


| Syntax | Description | Context |
| ------ | ----------- | ------- |
| `.globals` | Target specifier: the script is for globals.bin | Beginning of the file
| `.stage <name>` | Target specifier: the script is for a stage | Beginning of the file
| `.db <byte>, ...` | Embed raw bytes or text | After a label or other data
| `.dw <word>, ...` | Embed raw 16-bit words | After a label or other data
| `.dd <dword>, ...` | Embed raw 32-bit dwords | After a label or other data
| `.lib *<label>` | Declare a library function | `.globals` scripts
| `.prologue *<label>` | Declare a prologue function | `.stage` scripts
| `.startup *<label>` | Declare a startup function | `.stage` scripts
| `.dead *<label>` | Declare a player death handler | `.stage` scripts
| `.pose *<label>` | Declare a pose handler | `.stage` scripts
| `.time_cycle *<label>` | Declare a day-night cycle handler | `.stage` scripts
| `.time_up *<label>` | An odd event which never seems to be called | `.stage` scripts
| `.interact <obj>, *<label>` | Declare an object interaction event | `.stage` scripts

## Commands

These commands are available for use in script code. There are many different ways to use a command
and the arguments some commands accept can change based on the values of other arguments. Refer to
the existing scripts to find examples of these.

### Control Flow

| Name     | Description |
| -------- | ----------- |
| `break`  | Jump to a label after a `case` (equivalent to `goto`) |
| `case`   | Test a condition (equivalent to `if`) |
| `elif`   | Test a condition in an else branch (equivalent to `if`) |
| `endif`  | Jump to a label after an `if` (equivalent to `goto`) |
| `expr`   | Test an expression (equivalent to `if`) |
| `goto`   | Jump to a label |
| `if`     | Test a condition and jump if it's false |
| `lib`    | Call a library function in globals.bin |
| `return` | Return back to the caller |
| `run`    | Run a subroutine by its label |
| `while`  | Loop while a condition is true (equivalent to `if`) |

### Data Manipulation

| Name     | Description |
| -------- | ----------- |
| `popbp`  | Restore the stack pointer |
| `pushbp` | Save the stack pointer |
| `set`    | Assign or update a variable |
| `setsp`  | Push a value onto the stack |

### Cutscenes

| Name     | Description |
| -------- | ----------- |
| `anim`   | Animation control |
| `anim1`  | Animation control (?) |
| `anim2`  | Animation control (?) |
| `attach` | Attach a background subroutine to an object |
| `born`   | Spawn an object |
| `camera` | Camera control |
| `check`  | Test a game condition
| `color`  | Change an object's colors |
| `detach` | Remove a subroutine from an object |
| `dir`    | Rotate an object |
| `disp`   | Hide or show an object |
| `kill`   | Destroy an object |
| `light`  | Lighting control |
| `mdir`   | Advanced object rotation (?) |
| `move`   | Move an object |
| `moveto` | Advanced object movement (?) |
| `movie`  | Play a video file |
| `mscale` | Advanced object scaling (?) |
| `msg`    | Display a message |
| `pos`    | Move an object (?) |
| `ptcl`   | Particle effects |
| `scale`  | Scale an object |
| `scrn`   | Screen effects |
| `select` | Show a custom menu |
| `sfx`    | Sound effect control |
| `timer`  | Call a subroutine after time has elapsed |
| `wait`   | Wait for a game condition |
| `win`    | Message window control |

### Debugging

| Name     | Description |
| -------- | ----------- |
| `abort`  | Abnormally terminate script execution |
| `printf` | Log a debug message |

### Others

| Name     | Description |
| -------- | ----------- |
| `call`   | Invoke a system call, sometimes on an object |
| `menu`   | Show a predefined menu |
| `read`   | Load a resource into memory |
| `warp`   | Warp to a stage |

## Operands

Operands are arguments that can be passed to commands or directives. There are many different types:

| Type | Examples | Description |
| ---- | -------- | ----------- |
| Auto | `12345`<br>`0xabcdef` | Integer of any size |
| Byte | `1.b`<br>`0xcc.b` | 8-bit integer |
| Word | `12345.w`<br>`0xabcd.w` | 16-bit integer |
| Dword | `12345678.d`<br>`0xabcdef.d` | 32-bit integer |
| Type Code | `@anim` | Alters the semantics of a command |
| Text | `"Hello, world!"` | Text string (Latin-1/SHIFT-JIS) |
| Label | `*my_label` | Memory address of a label |
| Else Label | `else *my_label` | Label reference used to make `if` commands more readable |
| Offset | `*0x10` | Memory address of a file offset (typically used to read stage metadata) |
| Expression | `mul(1, 2)` | Complex expression (see below) |
| Message | `"Hello, world!"`<br>`speed(1)` | Message commands (see below) |

Whether or not an integer is interpreted as signed or unsigned is dependent on context, so Unplug
accepts both everywhere.

You may notice that most integers have type suffixes (`.b`, `.w`, `.d`). These indicate to the
assembler how many bytes an integer takes up. They are used to preserve the original code and you do
not need to use them in new code that you write (unless you want to), because the assembler can
determine how large a number should be based on context. 

## Expressions

_Expressions_ are special operands which can be passed to commands and other expressions to perform
complex calculations. Some expressions take no arguments, e.g. `money`, whereas others can take
several and some even accept type codes that alter their semantics. There is no practical limit to
how many expressions can be nested inside each other - some `if` statements in the game code are
extremely long!

Note that all expressions evaluate to 32-bit integers, as every value in a script is an integer.
There is no floating-point support and no native string type (they're just memory addresses under
the hood).

| Expression | Description |
| ---------- | ----------- |
| `eq(x, y)` | 1 if `x` is equal to `y`, 0 otherwise |
| `ne(x, y)` | 1 if `x` is not equal to `y`, 0 otherwise |
| `lt(x, y)` | 1 if `x` is less than `y`, 0 otherwise |
| `le(x, y)` | 1 if `x` is less than or equal to `y`, 0 otherwise |
| `gt(x, y)` | 1 if `x` is greater than `y`, 0 otherwise |
| `ge(x, y)` | 1 if `x` is greater than or equal to `y`, 0 otherwise |
| `not(x)` | Invert a condition (1 if false, 0 if true) |
| `add(x, y)` | `x + y` |
| `sub(x, y)` | `x - y` |
| `mul(x, y)` | `x * y` |
| `div(x, y)` | `x / y` |
| `mod(x, y)` | `x % y` |
| `and(x, y)` | Bitwise AND of `x` and `y` |
| `or(x, y)` | Bitwise OR of `x` and `y` |
| `xor(x, y)` | Bitwise XOR of `x` and `y` |
| `adda(x, y)` | `x += y` (only valid in `set`) |
| `suba(x, y)` | `x -= y` (only valid in `set`) |
| `mula(x, y)` | `x *= y` (only valid in `set`) |
| `diva(x, y)` | `x /= y` (only valid in `set`) |
| `moda(x, y)` | `x %= y` (only valid in `set`) |
| `anda(x, y)` | `x &= y` (only valid in `set`) |
| `ora(x, y)` | `x \|= y` (only valid in `set`) |
| `xora(x, y)` | `x ^= y` (only valid in `set`) |
| `sp(i)` | Value on the stack at `i` |
| `bp(i)` | Value on the parent stack at `i` |
| `flag(i)` | Value of global flag `i` |
| `var(i)` | Value of global variable `i` |
| `result` | Temporary variable holding the result of the last command |
| `result2` | Unused temporary variable |
| `pad(x)` | Gamepad state ([details](https://github.com/adierking/unplug/blob/8245f7defb8e8065e2f4bdfce320edb88d64a7af/unplug/src/event/expr.rs#L111)) |
| `battery(x)` | Player's current (`x` = 0) or max (`x` = 1) battery level in hundredths of watts |
| `money` | Player's moolah count |
| `item(id)` | Inventory count of item `id` |
| `atc(id)` | 1 if attachment `id` is unlocked, 0 otherwise |
| `rank` | Player's chibi-ranking |
| `exp` | Player's happy point count |
| `level` | Player's upgrade level (14 = super) |
| `hold` | ID of the held item |
| `map(x)` | Current (`x` = 0) or previous (`x` = 1) map ID |
| `actor_name(id)` | Display name of actor `id` |
| `item_name(id)` | Display name of item `id` |
| `time(x)` | In-game time data:<br>`x` = 0: day/night flag<br>`x` = 1: time as a percentage from 0-100<br>`x` = 2: time rate |
| `cur_suit` | Currently-equipped suit |
| `scrap` | Player's scrap count |
| `cur_atc` | Currently-equipped attachment |
| `use` | ID of the item that triggered an interaction |
| `hit` | ID of a projectile that triggered an interaction |
| `sticker_name(id)` | Display name of sticker `id` |
| `obj(...)` | Object properties |
| `rand(max)` | Random number between 0 and `max` (inclusive) |
| `sin(x)` | Sine of `x` in hundredths of degrees |
| `cos(x)` | Cosine of `x` in hundredths of degrees |
| `array(type, index, address)` | Access an array element ([details](https://github.com/adierking/unplug/blob/8245f7defb8e8065e2f4bdfce320edb88d64a7af/unplug/src/event/expr.rs#L965)) |

## Message Commands

The `msg` and `select` commands do not accept expressions like other commands do - rather, they
accept a sequence of _message commands_. These commands can either be text strings to display or
special modifiers which change the appearance or behavior of the text:

| Command | Description |
| ------- | ----------- |
| `anim(flags, obj, anim)` | Animation control |
| `ask(flags, default)` | Ask the player a yes/no question |
| `center(x)` | If `x` is 1, center the text, otherwise left-align it |
| `color(i)` | Use predefined color `i` |
| `def(flags, i)` | Set the default item in a `select` command |
| `format(str)` | A printf format specifier which reads values off the stack |
| `icon(i)` | Display predefined icon `i` |
| `input(digits, editable, selected)` | Ask the player for a number |
| `prop(x)` | If `x` is 1, use proportional text spacing, otherwise monospace |
| `rgba(color)` | RGBA color |
| `rotate(deg)` | Character rotation |
| `scale(x, y)` | Text scale |
| `sfx(...)` | Sound effect control |
| `shake(flags, strength, speed)` | Shaky text |
| `size(x)` | Text size (22 = default, 255 = reset) |
| `speed(x)` | Text speed (2 = default, higher is slower) |
| `stay` | Keep the message on-screen after it's done |
| `voice(id)` | Voice selection |
| `wait(x)` | Wait for a certain amount of time or a button press (254, 255) |
