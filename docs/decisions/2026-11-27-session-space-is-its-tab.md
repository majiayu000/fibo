---
status: accepted
date: 2026-11-27
decision-makers: majiayu000
---

# 会话算在它所在标签的空间，不再按目录挪到更窄的项目空间

## Context

11-03 的 `7280ec3e` 做了两件事：在宽空间（如工作空间 = `~`）里开、却在有自己空间的项目里做的会话，侧栏算到那个项目的空间；新标签开在这种目录时直接放进那个空间。

前一半在 11-27 出了问题。工作空间的一个标签左右分屏，两边都 `cd` 到 mooring 跑 Claude，侧栏把这两条列在 mooring 下，但 mooring 的标签栏里没有它们。用户原话：「mooring 空间下只有 2 个 tab？但是左侧就有 3 个 session」「现在非常混乱」。侧栏用的是 `session_space`（按目录），标签栏用的是 `terminals.space_id`，同一件事有两个答案。

## Decision

- `Workspace::session_space`：有待办的看待办，有终端的看终端所在的空间（也就是能在哪个空间的标签里找到它），两样都没有（Fibo 没开过的会话）才按目录找最深的空间。
- 11-03 的后一半不动：新标签开在有自己空间的项目目录里，标签直接放进那个空间（`fibo-app/src/host/store.rs` 的 `narrower_space`）。按目录归空间的主要场景走这条路，不会出现标签和侧栏对不上。

不做：自动把整个标签搬进项目空间（会挪动用户的标签，分屏两边目录不同时也没法搬）；侧栏行上加「在工作空间里」的标注（两个答案还在，只是写出来了）。

## Consequences

- 在宽空间的标签里 `cd` 进项目跑的会话，侧栏和手机都列在宽空间下；想归到项目，就在项目空间开新标签。
- 只改判定，不改存储：已有的会话换版本后立刻按新规则显示。另一台电脑的行由那台的后台算，那台也要换成新版。

## Verification

`cargo test -p fibo-core --lib a_session_is_listed_in_the_space_whose_tab_holds_it`

## More Information

推翻 `7280ec3e` 的前一半。重开条件：用户要求在宽空间里开的会话按目录归项目，并接受标签栏对不上（或者同时给出搬标签的做法）。
