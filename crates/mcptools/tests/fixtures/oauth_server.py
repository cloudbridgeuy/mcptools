import base64
import hashlib
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

log, mode, browser_log = sys.argv[1:]

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
        if path == '/.well-known/oauth-protected-resource/mcp':
            self.send(200, {'resource': base + '/mcp', 'authorization_servers': [issuer]})
        elif path in (('/tenant/.well-known/openid-configuration',) if mode == 'oidc-path' else ('/.well-known/oauth-authorization-server', '/.well-known/openid-configuration')):
            metadata = {'issuer': issuer, 'authorization_endpoint': base + '/authorize', 'token_endpoint': base + '/token', 'code_challenge_methods_supported': ['S256']}
            if mode != 'no-registration':
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
            self.record({'method': 'MCP', 'authorization': auth})
            if auth != 'Bearer access-secret-refreshed' and (not auth or mode in ('expired', 'unauthorized', 'refresh-fail')):
                self.send(401, headers={'WWW-Authenticate': 'Bearer resource_metadata="' + base + '/.well-known/oauth-protected-resource/mcp"'})
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
            self.record({'method': 'register', 'body': json.loads(body)})
            if mode == 'public-client':
                self.send(201, {'client_id': 'registered-client', 'token_endpoint_auth_method': 'none'})
            else:
                self.server.secrets.add('client-secret-value')
                self.send(201, {'client_id': 'registered-client', 'client_secret': 'client-secret-value', 'token_endpoint_auth_method': 'client_secret_post'})
        elif path == '/token':
            form = {key: values[0] for key, values in parse_qs(body).items()}
            self.record({'method': 'token', 'form': form})
            if form.get('grant_type') == 'refresh_token':
                if mode == 'refresh-fail':
                    self.send(400, {'error': 'invalid_grant', 'error_description': 'refresh-secret-value'})
                else:
                    self.server.secrets.add('access-secret-refreshed')
                    self.send(200, {'access_token': 'access-secret-refreshed', 'refresh_token': 'refresh-secret-value', 'token_type': 'Bearer', 'expires_in': 3600})
            else:
                with open(browser_log) as file:
                    authorization = json.load(file)
                challenge = base64.urlsafe_b64encode(hashlib.sha256(form.get('code_verifier', '').encode()).digest()).decode().rstrip('=')
                if challenge != authorization.get('code_challenge') or authorization.get('code_challenge_method') != 'S256':
                    self.send(400, {'error': 'invalid_grant'})
                    return
                self.send(200, {'access_token': 'access-secret-initial', 'refresh_token': 'refresh-secret-value', 'token_type': 'Bearer', 'expires_in': 0 if mode == 'expired' else 3600})
        else:
            self.send(404)

    def do_DELETE(self):
        self.send(204)

server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
server.secrets = {'access-secret-initial', 'refresh-secret-value'}
print(server.server_port, flush=True)
server.serve_forever()
