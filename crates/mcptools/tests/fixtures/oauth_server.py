import base64
import hashlib
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

log, mode, browser_log = sys.argv[1:]
slack = mode.startswith('slack')
redirect = 'https://oauth.example.test/slack/return'

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send(self, status, body=None, headers=None):
        data = json.dumps(body).encode() if body is not None else b''
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(data)

    def record(self, event):
        with open(log, 'a') as file:
            file.write(json.dumps(event) + '\n')

    def do_GET(self):
        path = urlsplit(self.path).path
        base = 'http://127.0.0.1:' + str(self.server.server_port)
        issuer = base + '/tenant' if mode == 'oidc-path' else base
        self.record({'method': 'GET', 'path': path})
        if path == ('/.well-known/oauth-protected-resource' if mode == 'host-resource' else '/.well-known/oauth-protected-resource/mcp'):
            self.send(200, {'resource': base if mode == 'host-resource' else base + '/mcp', 'authorization_servers': [issuer]})
        elif path in (('/tenant/.well-known/openid-configuration',) if mode == 'oidc-path' else ('/.well-known/oauth-authorization-server', '/.well-known/openid-configuration')):
            metadata = {'issuer': issuer, 'authorization_endpoint': base + '/authorize', 'token_endpoint': base + '/token', 'code_challenge_methods_supported': ['S256']}
            if slack:
                metadata['token_endpoint_auth_methods_supported'] = ['client_secret_basic'] if mode == 'slack-basic' else ['client_secret_post']
            if mode != 'no-registration' and not slack:
                metadata['registration_endpoint'] = base + '/register'
            self.send(200, metadata)
        else:
            self.send(404)

    def do_POST(self):
        base = 'http://127.0.0.1:' + str(self.server.server_port)
        body = self.rfile.read(int(self.headers.get('Content-Length', '0'))).decode()
        path = urlsplit(self.path).path
        if path == '/mcp':
            auth = self.headers.get('Authorization', '')
            self.record({'method': 'MCP', 'path': path, 'authorization': auth})
            if auth != 'Bearer access-secret-refreshed' and (not auth or mode in ('expired', 'unauthorized', 'refresh-fail', 'host-resource') or slack):
                metadata_path = '/.well-known/oauth-protected-resource' if mode == 'host-resource' else '/.well-known/oauth-protected-resource/mcp'
                self.send(401, headers={'WWW-Authenticate': 'Bearer resource_metadata="' + base + metadata_path + '"'})
                return
            request = json.loads(body)
            method = request['method']
            if method.startswith('notifications/'):
                self.send(202)
            elif method == 'initialize':
                self.send(200, {'jsonrpc': '2.0', 'id': request['id'], 'result': {'protocolVersion': '2025-06-18', 'capabilities': {'tools': {}}}}, {'Mcp-Session-Id': 'test-session'})
            elif method == 'tools/list':
                self.send(200, {'jsonrpc': '2.0', 'id': request['id'], 'result': {'tools': [{'name': 'echo', 'description': ' '.join(self.server.secrets), 'inputSchema': {'type': 'object'}}]}})
            else:
                self.record({'method': 'tool-call'})
                self.send(200, {'jsonrpc': '2.0', 'id': request['id'], 'result': {'content': [{'type': 'text', 'text': ' '.join(self.server.secrets)}]}})
        elif path == '/register':
            if slack:
                self.send(404)
                return
            self.record({'method': 'register', 'body': json.loads(body)})
            if mode == 'public-client':
                self.send(201, {'client_id': 'registered-client', 'token_endpoint_auth_method': 'none'})
            else:
                self.server.secrets.add('client-secret-value')
                self.send(201, {'client_id': 'registered-client', 'client_secret': 'client-secret-value', 'token_endpoint_auth_method': 'client_secret_post'})
        elif path == '/token':
            form = {key: values[0] for key, values in parse_qs(body).items()}
            self.record({'method': 'token', 'form': form})
            if mode == 'host-resource':
                with open(browser_log) as file:
                    authorization = json.load(file)
                if form.get('resource') != base or authorization.get('resource') != base:
                    self.send(400, {'error': 'invalid_target'})
                    return
            if slack:
                refresh = form.get('grant_type') == 'refresh_token'
                secret = 'rotated-client-secret-value' if refresh and mode == 'slack-rotate' else 'configured-client-secret-value'
                expected = 'Basic ' + base64.b64encode(('configured-client:' + secret).encode()).decode()
                authenticated = self.headers.get('Authorization') == expected if mode == 'slack-basic' else form.get('client_secret') == secret and form.get('client_id') == 'configured-client'
                if not authenticated or form.get('redirect_uri') != redirect:
                    self.send(200, {'ok': False, 'error': 'bad_client_secret'})
                    return
                with open(browser_log) as file:
                    authorization = json.load(file)
                if authorization.get('redirect_uri') != redirect:
                    self.send(200, {'ok': False, 'error': 'bad_redirect_uri'})
                    return
                if mode == 'slack-rotate' and refresh:
                    self.server.secrets.discard('configured-client-secret-value')
                    self.server.secrets.add(secret)
                if mode == 'slack-error':
                    self.send(200, {'ok': False, 'error': 'invalid_code', 'access_token': 'access-secret-initial', 'token_type': 'user'})
                    return
            if form.get('grant_type') == 'refresh_token':
                if mode == 'refresh-fail':
                    self.send(400, {'error': 'invalid_grant', 'error_description': 'refresh-secret-value'})
                else:
                    self.server.secrets.discard('access-secret-initial')
                    self.server.secrets.add('access-secret-refreshed')
                    self.send(200, {'ok': True, 'access_token': 'access-secret-refreshed', 'refresh_token': 'refresh-secret-value', 'token_type': 'user' if slack else 'Bearer', 'expires_in': 3600, 'authed_user': {'id': 'U12345', 'scope': 'read,write'}})
            else:
                with open(browser_log) as file:
                    authorization = json.load(file)
                challenge = base64.urlsafe_b64encode(hashlib.sha256(form.get('code_verifier', '').encode()).digest()).decode().rstrip('=')
                if challenge != authorization.get('code_challenge') or authorization.get('code_challenge_method') != 'S256':
                    self.send(400, {'error': 'invalid_grant'})
                    return
                self.send(200, {'ok': True, 'access_token': 'access-secret-initial', 'refresh_token': 'refresh-secret-value', 'token_type': 'user' if slack else 'Bearer', 'expires_in': 0 if mode == 'expired' else 3600, 'authed_user': {'id': 'U12345', 'scope': 'read,write'}})
        else:
            self.send(404)

    def do_DELETE(self):
        self.send(204)

server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
server.secrets = {'access-secret-initial', 'refresh-secret-value'}
if slack:
    server.secrets.add('configured-client-secret-value')
print(server.server_port, flush=True)
server.serve_forever()
