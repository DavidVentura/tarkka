# Tarkka

This is an application that takes Kaikki data (JSON dumps of multi-lingual dictionaries) and converts it to monolingual dictionaries, while also drastically chopping down the data contained.

For example, the "Spanish articles" file from Kaikki is 1.1GiB, that is, definitions in Spanish, for many languages.

Just compressing the raw Kaikki file with `zstd -9` becomes 59MiB, but without an index, you'd need to read & decode the whole 1.1GiB of JSON to find the word `Zorro`.

Trimming down & compressing that data with `tarkka` becomes 12MiB; though for fast access, I've added indices, making it bloat up to 21MiB.

The raw files from `Kaikki` can be obtained from sites such as: `https://kaikki.org/itwiktionary/rawdata.html`

## Dictionary File Format

This document describes the binary format used by Tarkka dictionary files.

## File Structure Overview

```
┌─────────────────┐
│     HEADER      │  32 bytes
├─────────────────┤
│  LEVEL 1 index  │  Variable size (3-byte prefix index)
├─────────────────┤
│  LEVEL 2 index  │  Variable size (compressed word groups with prefix compression)
├─────────────────┤
│  BINARY DATA    │  Variable size (compressed custom binary serialization)
└─────────────────┘
```

## Header Format (32 bytes)

```
Byte Offset:  0   1   2   3
            ┌───┬───┬───┬───┐
         0  │ D │ I │ C │ T │  Magic signature
            ├───┼───┼───┼───┤
         4  │ Level 1 Size  │  32-bit LE
            ├───┼───┼───┼───┤
         8  │ Level 2 Size  │  32-bit LE
            ├───┼───┼───┼───┤
        12  │  Word Count   │  32-bit LE
            ├───┼───┼───┼───┤
        16  │   Timestamp   │  64-bit LE (Unix seconds)
            ├───┼───┼───┼───┤
        24  │Ver│ Reserved  │  1 byte version + 7 reserved
            └───┴───┴───┴───┘
```

- **Magic**: 4-byte signature "DICT" (0x44494354)
- **Level 1 Size**: 32-bit little-endian size of Level 1 data in bytes
- **Level 2 Size**: 32-bit little-endian size of Level 2 data in bytes
- **Word Count**: 32-bit little-endian total number of words in dictionary
- **Timestamp**: 64-bit little-endian Unix timestamp (creation time)
- **Version**: 1-byte format version number
- **Reserved**: 7 bytes reserved for future use

## Level 1 Format

Level 1 contains an index mapping 3-byte prefixes to Level 2 group locations.

The level 1 index is not compressed, and <1MiB in size.

```
Entry Format:
┌─────────────────┬───────────────┬─────────────────┐
│   3-Byte Key    │  Raw L2 Size  │ Binary Offset   │
│   (3 bytes)     │   (4 bytes)   │   (4 bytes)     │
└─────────────────┴───────────────┴─────────────────┘
```

## Level 2 Format

Level 2 contains zstd-compressed groups of words sharing the same 3-byte prefix, using prefix compression within each group.

```
┌────────────┬────────────┬──────────┬──────┬────────────┬────────────┬──────────┬──────┬───
│ Shared Len │ Suffix Len │  Suffix  │ Size │ Shared Len │ Suffix Len │  Suffix  │ Size │ ...
│   (1 B)    │   (1 B)    │(Suffix B)│(2 B) │   (1 B)    │   (1 B)    │(Suffix B)│(2 B) │
└────────────┴────────────┴──────────┴──────┴────────────┴────────────┴──────────┴──────┴───
 Entry 1                                     Entry 2
```

Word reconstruction: `previous_word[0:shared_len] + suffix`

## Binary Data Format

The binary data section contains zstd-compressed compact binary serialization of word definitions and metadata.

- Numbers are serialized little-endian
- Vectors are serialized as `<length><data>` where length encoding depends on the field:
  - **u8 length**: 1 byte (0-255) for small collections
  - **u16 length**: 2 bytes little-endian (0-65535) for medium collections
  - **VarUint length**: 1-2 bytes for optimized encoding (0-32767)
- Strings are serialized as UTF-8 with VarUint length prefix

## VarUint Encoding

The first bit indicates whether it's a one-byte value or a two-byte value.

## File Layout Example

```
┌────────────────────────┐ ← 0x00
│ "DICT" │ 1024 │ 8192 │ │   Header (32 bytes)
│ 12000 │ timestamp │ v1 │   Magic, sizes, count, timestamp, version
├────────────────────────┤ ← 0x20 (32)
│  000│a │ 0256│ 0000    │   Level 1 data (1024 bytes)
│  00a│b │ 0128│ 0256    │   3-byte prefix → L2 size + Binary offset
│  abc│d │ 0512│ 0384    │
│  ...                   │
├────────────────────────┤ ← 0x420 (1056)
│  zstd-compressed       │   Level 2 index (8192 bytes)
│  prefix-compressed     │   Word entries per 3-byte group
│  word groups           │   with shared prefix optimization
├────────────────────────┤ ← 0x2420 (9248)
│  zstd-compressed       │   Word data
│  word data             │   
└────────────────────────┘
```
