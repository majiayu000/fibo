//! The ops a machine may list in `hello`. `image` keeps a picture for a later `send`;
//! `steer` says `send` takes `how` (`queue` or `steer`, see the host's `turn_input`); `effort`
//! says `modes` and `setmode` take the kind `effort`; `spacequiet` puts a space aside or takes it
//! back, and `list` names the ones put aside in `quiet`. `sendnow` sends what the Agent has
//! queued without waiting for the step at hand; `toolslast` says `toolcalls` takes `last`;
//! `openimages` says `open` takes `images`, pictures for the new Agent's first message.

/// Every op, in the order `hello` lists them.
pub const OPS: &[&str] = &[
    "hello",
    "list",
    "cards",
    "read",
    "send",
    "keys",
    "input",
    "answer",
    "ask",
    "trust",
    "chat",
    "events",
    "watch",
    "resize",
    "open",
    "end",
    "outcome",
    "close",
    "ping",
    "diff",
    "dirs",
    "mkdir",
    "stop",
    "quota",
    "lastsession",
    "addspace",
    "usage",
    "toolcalls",
    "toolcall",
    "backfill",
    "todoadd",
    "todooutcome",
    "suggestion",
    "modes",
    "setmode",
    "image",
    "steer",
    "effort",
    "spacequiet",
    "sendnow",
    "toolslast",
    "openimages",
];
