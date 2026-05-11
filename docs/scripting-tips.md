# Scripting Tips 'n Tricks

This documents various tricks you can use to write scripts in Unplug's assembly language. Feel free
to make a pull request and add to this document if you have any generally useful tips.

## In-Game Debug Menu

There's a hidden debug menu you can access using this AR code that replaces the pause menu:

```text
0000FEEF 00000084
```

(You can also show it from a script using `menu 0`.)

The options at the bottom of the debug menu are actually built up in globals lib 375, so this makes
it a convenient way to run one-off scripts.

You can add up to five options to it (including the one already there by default). Simply add
another label for a raw byte string to use as the item text, then do a `call -200` with the name
pointer and a pointer to the script to run when it is selected.

```text
    .lib   375, *debug_menu
debug_menu:
    call   -200, *option_1_text, *option_1
    call   -200, *option_2_text, *option_2
    call   -200, *option_3_text, *option_3
    call   -200, *option_4_text, *option_4
    call   -200, *option_5_text, *option_5
    return

option_1_text:
    .db    "My Option 1"
option_1:
    msg    "Option 1!", wait(254)
    return

option_2_text:
    .db    "My Option 2"
option_2:
    msg    "Option 2!", wait(254)
    return

option_3_text:
    .db    "My Option 3"
option_3:
    msg    "Option 3!", wait(254)
    return

option_4_text:
    .db    "My Option 4"
option_4:
    msg    "Option 4!", wait(254)
    return

option_5_text:
    .db    "My Option 5"
option_5:
    msg    "Option 5!", wait(254)
    return
```

## Expressions in Messages

To print expressions in messages, you need to create a stack frame (`pushbp`), push each value onto
the stack (`setsp`) **in reverse order**, access them using `format()`, then restore the stack frame
(`popbp`).

This code will print "1, 2, 3!":

```text
pushbp
setsp  3
setsp  2
setsp  1
msg    format("%d"), ", ", format("%d"), ", ", format("%d"), "!", wait(254)
popbp
```

This accepts standard `printf()` format strings.

**Note:** You cannot access more than one value at a time with a single `format()` command. Trying
to use `format("%d, %d, %d!")` will print garbage. Future versions of Unplug may make this easier to
work with by emitting `format` commands automatically when you use `%` in a message.

## Reading Arbitrary Memory

**Requires a nightly build of Unplug.**

You can actually read arbitrary memory addresses using the `array()` function because the pointer
value is just an expression. However, it was only intended for script data, so it reads multi-byte
values in little-endian order. To read a big-endian value, you have to piece the bytes together
manually.

The following code will display how long you've been playing the current save, which is not normally
accessible to scripts:

```text
set    var(1), 0x8035b600                                 ; Playtime address
set    var(0), array(1, 0, var(1))                        ; Load byte 0 (MSB)
set    var(0), or(array(1, 1, var(1)), mul(var(0), 256))  ; Load byte 1
set    var(0), or(array(1, 2, var(1)), mul(var(0), 256))  ; Load byte 2
set    var(0), or(array(1, 3, var(1)), mul(var(0), 256))  ; Load byte 3

set    diva(var(0), 60)          ; Convert frames to seconds
pushbp
setsp  mod(var(0), 60)           ; Seconds = total % 60
setsp  mod(div(var(0), 60), 60)  ; Minutes = total / 60 % 60
setsp  div(var(0), 3600)         ; Hours   = total / 3600
msg    "Your playtime: ", format("%02d"), ":", format("%02d"), ":", format("%02d"), wait(254)
popbp
```

(It's not necessary to use a variable for the address if you don't want to, but it saves having to
repeat it here.)
