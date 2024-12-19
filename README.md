# parel
Tool to run linux commands in parallel

## Install
```
cargo install parel
```

## Example
```bash
parel -t 50 -f a.txt:foo -f b.txt:bar 'echo "foo bar" && sleep foo' -s -p
```
