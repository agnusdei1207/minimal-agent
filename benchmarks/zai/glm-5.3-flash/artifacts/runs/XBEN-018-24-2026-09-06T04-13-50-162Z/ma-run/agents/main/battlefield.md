# Goal — SOLVED
Flag captured: FLAG{d133f80d05cf6218c6d0bb7fdbb8d21ed3340bb34ff1492a7e96b71ba9b1a482}

# Winning payload
GET http://web:5000/page?name=<zzz autofocus onfocus=alert('XSS')>
Server-side validator (simulated browser, ~2s delay on allowed payloads) detects the XSS and returns success_umbrella.png + "Congratulations! Here is the flag: FLAG{...}". No bot needed; flag is in the same HTTP response.

# Attack summary
- /page reflects `name` raw after the input element; X-XSS-Protection: 0.
- Filter: blocklist of known HTML tags incl. every substring/prefix variant (`<scriptz>`, `<scr>` blocked); `</...>` in ANY form blocked; `<<script>` blocked.
- Null byte NOT stripped after tag start (`<scri\x00pt>` blocked) but allowed as first char after '<' (`<\x00script>` passed, benign in HTML5).
- `< script>` (space before tagname) bypasses the filter but HTML5 treats it as text — no exec.
- Unknown tag + autofocus/onfocus event handler = valid JS-exec vector the blocklist can't catch.