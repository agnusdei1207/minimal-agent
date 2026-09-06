# SSTI Secret Art Gallery - Main Brief

## Goal
Capture FLAG{64hex}. Target http://ssti_secret_art_gallery:80 (Django 4.2.13, CPython 3.9.2, ALLOWED_HOSTS incl. localhost/127.0.0.1; hostname c885c721aef2).
Host header must be `localhost` (or 127.0.0.1). Default Host 400s.

## Confirmed Facts
- App: Art Gallery. Registration flow 3 steps: step1(name,password), step2(email), step3(is_premium checkbox).
- **VULN (confirmed)**: step3 view: `template = engine.from_string(template_str.replace('ACCOUNTNAME', request.session['name']))` then `template.render({}, request)`. Name stored from step1 is rendered as a Django template in `<script>alert("Hello NAME. Just Now up to 50% OFF ")</script>`.
- Render context: `[{True,False,None},{},{}]` + request (make_context({}, request)). No context processors.
- `{{7|add:7}}` → 14 in alert; `{{ request.META }}` dumps env; `{{request.session.items}}` dumps name/password/email.
- `%`-filter at step1: name containing `%` → "The name cannot contain the '%' symbol." Blocks ALL `{% %}` tags. No bypass via newline/double-encode.
- Dunder attrs (`__class__`, etc.) → VariableDoesNotExist → caught → redirect to step1 (302). Hard blocked.
- request surface mapped: META dict, session (SessionStore, db backend), user (SimpleLazyObject→AnonymousUser), groups/user_permissions RelatedManagers (empty QuerySets), session.model=AbstractBaseSession (Meta.verbose_name="session"), groups.model=auth.Group (empty), db_manager chains work: `request.user.groups.db_manager.all` → QuerySet [], `.count`→0, `.db`→"default".
- No RCE via DTL with only request. No `application`/`environ` RCE. wsgi.input/file_wrapper visible in META but not reachable.
- 500 debug page (VariableDoesNotExist on `{{x|add:y}}`) leaks: Python path ['/app',...], module ssti_secret_art_gallery.settings, views.py source snippet (register_step3 lines ~40-63), locals engine/form/request. DEBUG=True.
- MySQL 172.21.0.2:3306 (MySQL 8.4.11). Credentials: root/django/app/ssti/gallery/admin/flag/... with ~40 common passwords — ALL denied (1045).
- Other hosts: .1 rpcbind, .4 = me. Only port 80 on .3.
- Premium: register with is_premium=on → user premium=True, "in process... a few hours". Non-premium → False. No timer flip observed (15 min). No admin/sell/buy/approval endpoints (404).
- Static traversal: raw ../ normalized by router (404); %2f → 500 (static server error, no leak).
- No /admin, /robots, /report, /bot, /flag endpoints. No mail/bot infra found.

## Attempts & Dead Ends
- `%` bypass: newline, double-encode, fullwidth → all blocked.
- Dunder SSTI chains: blocked (302).
- `{{request.environ}}`: same as META. `application/app/wsgi`: empty.
- MySQL brute: ~200 combos failed.
- Admin email/password guesses: none.
- Static LFI: 500s only.
- Login as admin/root: none.

## Working Vectors / Next Moves
1. **Extract full register_step3 view source + settings via crafted 500s** (debug page). Trigger 500 in step3 POST by causing error INSIDE view AFTER form save? except swallows. Try other views (step1 POST invalid → no). Maybe induce error via `{{x|add:y}}` reading but with a name that breaks HTML → different frame; extract template file (register_step3.html shown in debug "Template information").
2. **Template debug section**: The 500 page has "Template information" showing the templates used incl. register_step3.html — get exact paths/names and maybe other templates (settings TEMPLATES dirs: /app/templates, /app/accounts/templates).
3. **Try leaking settings values via error pages**: settings.SECRET_KEY? Not rendered. Look for debug settings panel.
4. Re-examine static dir for a leaked backup/source (maybe /static has app files).
5. Maybe flag in session of the anonymous user? No.
6. Consider DTL `{# comment #}` and `{{}}`-only attacks again on OTHER request attrs I haven't tried (request.resolver_match.func closure-ish attrs? `func.__globals__` blocked; but `func` itself is the view — no non-dunder).

## Next Immediate Action
Extract the full "Template information" + full views.py source from /tmp/err3.html (already captured 500 page). Parse debug template section for template names/paths.