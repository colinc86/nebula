# Challenge Project

This is a Go task management system with a benchmark challenge.

## If .neb files are present

The `.neb` files are **compact representations** of the Go source — same logic, fewer tokens. They use a terse syntax called Nebula where keywords are shortened (`fn`=func, `rt`=return, `st`=struct, `mc`=switch, `sf`=receiver, `fo`=for, `el`=else, `lt`=let, `mt`=mut, `tt`=true, `ff`=false, `bk`=break, `ct`=continue, `en`=enum, `im`=impl).

**Read the `.neb` files** to understand the codebase. They contain the same code as the `.go` files, just compressed. **Write your changes to the `.go` files** in normal Go.

## Tests

```sh
go test -v -count=1 ./...
```
