# Generate a 30-day token

```sh
admin/generate_token.sh NAME SERVER X Y     # e.g. admin/generate_token.sh alice US1 509 405
```

Params: `NAME` customer label (must be new), `SERVER` `US1` or `WORLD2`, `X Y` their main-castle coordinates on that server. It prints the token and saves it to `admin/NAME-30-day.token`.



admin/generate_token.sh great-emperor-8 WORLD2 677 688
