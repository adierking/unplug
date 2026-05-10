# Scripting Tips 'n Tricks

This documents various tricks you can use to write scripts in Unplug's assembly language. Feel free
to make a pull request and add to this document if you have any generally useful tips.

## In-Game Debug Menu

There's a hidden debug menu you can access using this AR code that replaces the pause menu:

```text
0000FEEF 00000084
```

The options at the bottom of the debug menu are actually built up in globals lib 375, so this makes
it a convenient way to run one-off scripts.

You can add up to five options to it (including the one already there by default). Simply add
another label for a raw byte string to use as the item text, then add a pointer to a script to run
when it is selected.

```text
option_1_text:
    .db    "My Option 1"
option_2_text:
    .db    "My Option 2"
option_3_text:
    .db    "My Option 3"
option_4_text:
    .db    "My Option 4"
option_5_text:
    .db    "My Option 5"

option_1:
    msg    "Option 1!", wait(254)
    return

option_2:
    msg    "Option 2!", wait(254)
    return

option_3:
    msg    "Option 3!", wait(254)
    return

option_4:
    msg    "Option 4!", wait(254)
    return

option_5:
    msg    "Option 5!", wait(254)
    return

    .lib   375.w, *lib_375
lib_375:
    call   -200.d, *option_1_text, *option_1
    call   -200.d, *option_2_text, *option_2
    call   -200.d, *option_3_text, *option_3
    call   -200.d, *option_4_text, *option_4
    call   -200.d, *option_5_text, *option_5
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

**Note:** You cannot access more than one value at a time with a single `format()` command. Trying
to use `format("%d, %d, %d!")` will print garbage. Future versions of Unplug may make this easier to
work with by emitting `format` commands automatically when you use `%` in a message.

## Reading Arbitrary Memory

**Requires a nightly build of Unplug.**

You can actually read arbitrary memory addresses using the `array()` function because the pointer
value is just an expression. However, it was only intended for script data, so it reads multi-byte
values in little-endian order. To read a big-endian value, you have to piece the bytes together
manually.

The following code will show your happy point total by directly reading the game's memory. This is
for demonstration purposes only; use `exp` for a much simpler way to get the happy point total in
real script code.

```text
set    var(0), 0x8038f73c  ; Address to read
set    var(1), array(1, 0, var(0))                        ; Read byte 0 (most significant)
set    var(1), or(array(1, 1, var(0)), mul(var(1), 256))  ; Read byte 1
set    var(1), or(array(1, 2, var(0)), mul(var(1), 256))  ; Read byte 2
set    var(1), or(array(1, 3, var(0)), mul(var(1), 256))  ; Read byte 3 (least significant)
pushbp
setsp  var(1)
msg    "Happy Points: ", format("%d"), wait(254)
popbp
```

(It's not necessary to use a variable for the address if you don't want to, but it saves having to
repeat it here.)
