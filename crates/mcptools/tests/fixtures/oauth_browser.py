#!/usr/bin/env python3
import json
import os
import sys
import urllib.request
from urllib.parse import parse_qs, urlencode, urlsplit

url = sys.argv[-1]
parsed = urlsplit(url)
params = {key: values[0] for key, values in parse_qs(parsed.query).items()}
with open(os.environ['OAUTH_BROWSER_LOG'], 'w') as file:
    json.dump(params, file)
redirect = params['redirect_uri']
mode = os.environ.get('OAUTH_BROWSER_MODE', '')
if mode.startswith('slack'):
    if redirect == 'https://oauth.example.test/slack/return':
        redirect = 'http://127.0.0.1:' + os.environ['OAUTH_CALLBACK_PORT'] + '/slack/return'
if mode == 'wrong-path' or mode.startswith('slack'):
    try:
        urllib.request.urlopen(redirect + '-wrong?' + urlencode({'code': 'code-secret-value', 'state': params['state']}), timeout=2).read()
    except Exception as error:
        with open(os.environ['OAUTH_BROWSER_LOG'] + '.rejected', 'w') as file:
            file.write(str(error))
if mode == 'wrong-state' or mode.startswith('slack'):
    try:
        urllib.request.urlopen(redirect + '?' + urlencode({'code': 'code-secret-value', 'state': 'incorrect-state'}), timeout=2).read()
    except Exception as error:
        with open(os.environ['OAUTH_BROWSER_LOG'] + '.rejected', 'w') as file:
            file.write(str(error))
try:
    urllib.request.urlopen(redirect + '?' + urlencode({'code': 'code-secret-value', 'state': params['state']}), timeout=2).read()
except Exception:
    pass
