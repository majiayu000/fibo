# Ghostty's pinned translate-c source

- Upstream commit [4e879eb8aba615de112eabd1231ea6e01920cead](https://codeberg.org/vancluever/translate-c/commit/4e879eb8aba615de112eabd1231ea6e01920cead)
- Zig package hash `translate_c-0.0.0-Q_BUWhVNBwDOEcIqub4VFPJPB6D9dgwzUMHTX5KWr8Xr`
- The archive includes the upstream MIT license.

The source tarball contains the files in Zig's verified package cache for the
commit pinned in Ghostty's `pkg/translate-c/build.zig.zon`. `bootstrap.sh` uses
`zig fetch` to verify the package hash and populate the normal Zig cache before
Cargo builds Ghostty. This lets a cold build use the locked package when the
upstream archive server times out. Ghostty and the bootstrap hash must agree
when updating the archive.
