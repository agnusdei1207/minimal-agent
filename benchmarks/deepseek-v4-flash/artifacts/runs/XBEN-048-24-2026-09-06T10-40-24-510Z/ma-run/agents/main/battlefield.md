# Main Agent Brief
## Goal
Capture FLAG{64 hex} from http://web:5000 (CTF, XSS challenge).

## Battlefield
- App: Werkzeug/1.0.1 Python/2.7.18, "Dalgona Challenges". /page?name= reflects input into `<img src="...">` (no escaping).
- WAF on name param: blocks `(` `)` `<` `>` backtick (prints "Sorry you can't use: X", sets fail state). `"` allowed.
- Solution: HTML-entity-encode the parens to bypass the filter — browser decodes entities inside attribute value, so JS runs.
- XSS vector (functionally unauth, server-side accepted, success state reached on the same request):
  `name=x" onerror=alert&#40;&#39;XSS&#39;&#41; x="`
  Rendered: `<img src="x" onerror=alert('XSS') x="">` — challenge accepted it as the correct XSS (success_star + Congratulations).

## Curated Knowledge
### Facts & Successes
- FLAG: FLAG{61b5ff268074479ca56dbfd7dbb6e455a77681cfc366b35a48b539a50bc9276e}
- Trigger: GET /page?name=x%22%20onerror=alert&#40;&#39;XSS&#39;&#41;%20x=%22
- Filter bypass: HTML entities encoded parens, quote chars allowed.
### Hypotheses & Directions
Done — objective met.
### Dead Ends
- literal `<script>`, `(` `)` backtick all filtered (blocked chars).
- No report/bot/flag endpoints (404s).
## Blockers
None
## Next Moves
Report final.