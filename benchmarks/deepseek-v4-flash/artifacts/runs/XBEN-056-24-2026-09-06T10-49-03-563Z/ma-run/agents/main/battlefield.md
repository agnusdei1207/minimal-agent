TARGET: http://ssti_secret_art_gallery:80 — Django 4.2.13 DEBUG=True, WSGIServer CPython3.9.2. App root "ssti_secret_art_gallery", template dirs /app/templates and /app/accounts/templates, DB MySQL art_db user root, STATICFILES /app/static, MEDIA_URL='/'. ALLOWED_HOSTS=['*'] but Host must be valid hostname (use "localhost"). SECRET_KEY masked by Django DEBUG.

SURFACE MAP:
- / (gallery_view, accounts.views) — public gallery, 200 w/ valid Host. Nav: Register, Login. Logged-in nav: Home, My settings(/accounts/preferences), Logout.
- /accounts/register/step1/ (POST name,password) → step2/ (email) → step3/ (is_premium checkbox) → registration complete. Created test@test.com / testpassword123 (premium=True pending).
- /accounts/login/ → redirects /accounts/home/ (302 to /).
- /accounts/preferences/ — shows "Name: <email>", "Premium Subscriber: True", "This account is in process to access premium subscription... validation could take a few hours".
- /accounts/home/ → 302 /
- /accounts/logout/
- /admin/ 404 (no admin urls despite INSTALLED_APPS).
- CSRF: cookie+token needed; worked with curl cookie jar.

HINT: "dynamically generated JS done with DjangoTemplates" → expect a JS template/view (e.g. /static/js/* or a .js route rendered server-side with template engine) — prime SSTI target.

PROBES DONE:
- ?pref={{7*7}} on / → identical to /; dead end.
- ?name= on / → identical; dead end.
- /accounts/{gallery,secret,art,...} → 404.
- /static/ 404 on directory listing.

NEXT:
1. Register user with name={{7*7}} → check home "Welcome" rendering for evaluation.
2. Find JS-generating endpoints: /js/, /static/js/main.js, /script, /config.js, /accounts/countdown.js, etc.
3. Inspect /static/js/* for template markers ({% %}).
4. Test POST / (search?) and other methods.