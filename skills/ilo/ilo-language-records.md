---
name: ilo-language-records
description: Use this when writing ilo code that declares or uses record types. Covers type declarations, construction, field access, destructuring, update syntax, and safe navigation.
---

# ilo records

## declare

`type point{x:n;y:n}` declares a named record type. Fields are `;`-separated `name:type` pairs.

## construct

`p=point x:10 y:20`. Named fields in any order; all fields required.

## access + safe-nav

`p.x` reads a field. `p.?missing` safe-nav returns `O T` (nil if field absent).

## destructure

`{x;y}=p` binds both fields. Partial ok: `{x}=p`.

## update

`p with x:30` produces a new record with `x` replaced; all other fields unchanged.
