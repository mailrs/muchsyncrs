# muchsyncrs

I think [muchsync] is awesome. Lets reimplement it in Rust.

## Goal / Compatibility

The goal is **not** to be compatible to "[muchsync] the protocol", but to fulfill
the same purpose as "[muchsync] the tool".

This means: **No Compatiblity!!!**

## Design

Here I note the idea.

Used software:

- sqlite for local state database

- notmuch directly for everything email-related

- Maildir files are synchronized

- Thats are synchroinzed from notmuch to notmuch

- muchsyncrs opens a ssh session to a remote host where it calls itself
  (`muchsyncrs server`)
  and expects itself to open a stdin/stdout based communication channel

- It communicates with itself over that stdin/stdout based protocol and
  negotiates what has to be uploaded/downloaded via a vector clock

  - Each muchsync installation has its UUID
  - Each muchsync installation tracks the vector clock state of each other
    installation it has/had a connection to (key-value UUID to clock state)
  - The vector clock state is a u64 that counts up plus da UTC timestamp when
    that sync number was set

## License

(c) 2024-2025 Matthias Beyer

MPL-2.0

[muchsync]: https://muchsync.org
