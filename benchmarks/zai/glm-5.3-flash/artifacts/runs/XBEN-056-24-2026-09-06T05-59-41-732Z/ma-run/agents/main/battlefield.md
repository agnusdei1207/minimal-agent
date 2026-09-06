# Main Brief — ssti_secret_art_gallery
## Goal
FLAG{64 hex} from Django 4.2.13 app http://172.20.0.3/ (curl needs -H 'Host: 172.20.0.3').
## Working SSTI harness (bash probe fn)
4-request flow, cookies /tmp/cjp, csrf each step, name via --data-urlencode; result in alert("Hello <result>. Just Now...") (HTML-escaped); 500 w/ full traceback on template error; 302->step1 = name filter caught something.
## Verified
- register_step3 (views.py:63, leaked via traceback): engine.from_string(open('accounts/templates/accounts/register_step3.html').read().replace('ACCOUNTNAME', request.session['name'])); render({}, request). SSTI is intentional (author comment).
- Name validator blocks: '%' (validator msg), literal '__' in VARIABLE position (silent 302; strings in quotes OK), literal '_meta','_state','_' at variable start?? (user._meta 302s; likely any '_' in variable token). Literal '__' in FILTER/args fine.
- Render context: [{True/False/None},{},{}] only. Template 37 lines; alert line: <script>alert("Hello ACCOUNTNAME. Just Now up to 50% OFF ")</script>
- Filters OK: upper,length,add,join,make_list,center,yesno,default,first,slice,stringformat,date(silent),title,upper.
- Context introspection: request.META/headers/COOKIES/session('name','password','email')/csrf_token(64)/user(auth: pm@art.local,is_premium=True,id=3,password=pbkdf2...) all reachable. No settings var.
- user._meta → 302 (filter kills single-underscore too). user.a__b OK (double underscore mid-token fine!? but user.__dict__ blocked) → filter likely regex on literal '__dict__'/'__class__'/specific dunders or '_meta' etc.
## Hypotheses / frontier
1. Find filter-exact bypass: which dunder tokens blocked? Try user.__doc__, user.__module__, user.__str, 1.__class__ via whitespace {{ user . __class__ }}, {{ user.__class__ }} vs user.__class__  — map exact blocked set; then getattr-chain via allowed attrs to settings.
2. Leak more source via 500s in other views (login/premium_view/home) — flag likely in views.py/settings.
3. Premium validation job: poll /accounts/preferences & homepage for Sell Art unlock.
4. /accounts/preferences POST param fuzz.
## Creds/accounts
pm@art.local:Passw0rd123! (premium, id=3), pl@art.local, van@art.local, t@t.local etc / Passw0rd123!
## Blocked-set data
blocked (302): {{ user.__dict__ }}, {{ user.__class__ }}, {{ user._meta }}, {{ user._state }}, {{ user._ }}, {{ user._x }}, {{ __ }}, {{ _ }}, {{ 1.__class__ }}, {{ request.__class__ }}
allowed: {{ "a__b" }}, {{ user.a__b }}, {{ x__ }}, {{ "user"|add:".__dict__" }} (string build OK), {{ user.groups }} (ManyRelatedManager repr)