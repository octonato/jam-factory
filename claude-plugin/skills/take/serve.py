#!/usr/bin/env python3
"""Serves one factory root and archives a job on `POST /archive/<repo>/<id>`."""
import functools
import json
import os
import sys
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlsplit


def job_file(root, path):
    parts = urlsplit(path).path.split('/')
    if len(parts) != 4 or parts[:2] != ['', 'archive']:
        return None

    repo, job_id = unquote(parts[2]), unquote(parts[3])
    for name in (repo, job_id):
        if name in ('', '.', '..') or '/' in name or '\\' in name:
            return None

    path = os.path.join(root, repo, 'jobs', job_id, 'job.json')
    return path if os.path.isfile(path) else None


def archive(path):
    with open(path) as f:
        job = json.load(f)

    job['archived'] = True
    job['updated'] = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

    tmp = f'{path}.tmp'
    with open(tmp, 'w') as f:
        json.dump(job, f, indent=2)
        f.write('\n')
    os.replace(tmp, path)


class Handler(SimpleHTTPRequestHandler):
    def do_POST(self):
        origin = self.headers.get('Origin')
        if origin is not None and origin != f'http://{self.headers.get("Host")}':
            self.send_error(HTTPStatus.FORBIDDEN)
            return

        path = job_file(self.directory, self.path)
        if path is None:
            self.send_error(HTTPStatus.NOT_FOUND)
            return

        try:
            archive(path)
        except (OSError, ValueError) as err:
            self.send_error(HTTPStatus.INTERNAL_SERVER_ERROR, str(err))
            return

        self.send_response(HTTPStatus.NO_CONTENT)
        self.end_headers()


def main():
    root, port = sys.argv[1], int(sys.argv[2])
    handler = functools.partial(Handler, directory=root)
    ThreadingHTTPServer(('127.0.0.1', port), handler).serve_forever()


if __name__ == '__main__':
    main()
