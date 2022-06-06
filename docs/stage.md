# Stage JSON Format

This document describes the format of the JSON data used by the `stage` command.

## Root

At the root is an object which currently only contains one array:

| Name | Type | Description |
| ---- | ---- | ----------- |
| `objects` | array | The objects in the stage.

## Objects

These are the objects that are placed in the stage.

To add a new object, add an entry to the end of the list and set its ID to the last ID plus 1.
Make sure to at least set the `spawn` flag or else it won't show up.

Unplug intentionally makes it hard to delete objects because scripts refer to each object by its
index. You can deactivate an object by removing the `spawn` flag or adding the `disabled` flag.

| Name | Type | Description |
| ---- | ---- | ----------- |
| `id` | number | The object's index in the object list.
| `object` | string | The object type.<br>To see the available objects, use the `list objects` commands or go [here](/unplug-data/src/gen/objects.inc.rs).
| `position` | object | The object's `x`, `y`, and `z` coordinates in world space.
| `rotation` | object | The object's `x`, `y`, and `z` rotation in degrees.
| `scale` | object | The object's `x`, `y`, and `z` scale multipliers.
| `data` | number | An auxiliary data value that some objects use.
| `spawnFlag` | number | A per-level and per-object-type flag index which is used to control whether the object should spawn. Typically the purpose of this is to make it so that items don't respawn after you pick them up. Don't edit this unless you know what you're doing.<br>This can be `null` to indicate the object does not have a flag assigned.
| `variant` | number | Some objects have multiple variants that you can choose between. For example, soda cans use this to control which texture they get. For most objects this will just be `0`.
| `flags` | array | A list of strings which specifies flags that control the object's behavior (see below).
| `script` | number | The block ID of the entry point for the object's interaction script. **This cannot be edited yet**.<br>This can be `null` to indicate the object does not have a script assigned.<br>Any objects added to the list must set this to `null`.

## Object Flags

These are the flags that can go in the `flags` list on each object.

Common flags you might want on an object are `spawn`, `climb`, `clamber`, and `interact`.

| Name | Description |
| ---- | ----------- |
| `spawn` | The object spawns when the stage loads.
| `opaque` | The object can obscure the player without showing any silhouette.
| `blastthru` | The object allows blaster projectiles to pass through it.
| `radar` | The radar will point to the object if it is nearby.
| `intangible` | The object is not solid and other objects can pass through it.
| `invisible` | The object is drawn fully transparent. This isn't the same as the object not rendering at all because it may still obscure some objects and shadows.
| `toon` | The object is lit using a toon effect.
| `flash` | The object flashes like an item.
| `unlit` | It's unclear exactly what this does. It's some sort of weird rendering effect. The `unlit` name is tentative.
| `botcam` | The object always shows in the utilibot camera window.
| `explode` | The object can be destroyed with the blaster.
| `pushthru` | The object allows other objects to be pushed through it.
| `lowpri` | The object will not be prioritized in interactions. If this is not set and the player presses A close to the object, they will automatically walk up and interact with it.
| `reflect` | The object shows in the floor reflection.
| `pushblock` | The object blocks other objects from being pushed through it.
| `cull` | The object is culled when not being looked at (this is mainly a performance optimization).
| `lift` | The player can lift the object up.
| `climb` | The player can climb on the object.
| `clamber` | The player can clamber up to surfaces on the object.
| `ladder` | The player can climb up the object as a ladder.
| `rope` | The player can climb up the object as a rope.
| `stairs` | The object is a staircase (i.e. it has internal ledges). The object's `data` value indicates the height of each step.
| `fall` | The object will fall if it is pushed off a ledge.
| `grab` | The player can grab the object and push/pull it.
| `interact` | The object can be interacted with by walking up to it and pressing A.
| `touch` | The object responds to being touched by the player.
| `atc` | The object responds to attachments.
| `projectile` | The object responds to projectiles.
| `unk28` | ???
| `mirror` | The object shows in mirrors.
| `unk30` | ???
| `disabled` | The object is disabled and cannot be spawned.<br>Note that this does not fully work with all object types.