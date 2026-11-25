import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { after, before, describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = path.resolve(HERE, '..');
const REPO_ROOT = path.resolve(PKG_ROOT, '../../../../..');
const HTTPYAC_BIN = path.join(PKG_ROOT, 'node_modules/.bin', process.platform === 'win32' ? 'httpyac.cmd' : 'httpyac');

/** Runs a child process asynchronously so the in-process HTTP server stays responsive. */
function runAsync(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      ...options,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString('utf8');
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString('utf8');
    });
    child.on('error', reject);
    child.on('close', (status) => resolve({ status, stdout, stderr }));
  });
}

/** Returns true if `command` is executable on PATH. */
function hasBinary(command) {
  const probe = spawnSync(process.platform === 'win32' ? 'where' : 'which', [command], {
    stdio: 'ignore',
  });
  return probe.status === 0;
}

/** Resolves or builds the `forge` binary once for the test suite. */
function resolveForgeBinary() {
  if (process.env.FORGE_BINARY) {
    return process.env.FORGE_BINARY;
  }
  const build = spawnSync('cargo', ['build', '--quiet', '--bin', 'forge'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
  });
  assert.equal(build.status, 0, `cargo build --bin forge failed:\n${build.stderr}`);
  const exe = process.platform === 'win32' ? 'forge.exe' : 'forge';
  return path.join(REPO_ROOT, 'target/debug', exe);
}

/** Builds a comprehensive OpenAPI 3.1 spec pointing at `baseUrl`. */
function buildSpec(baseUrl) {
  const largeProps = {
    name: { type: 'string', default: 'block-bots' },
    action: { type: 'string', enum: ['block', 'allow', 'log'] },
  };
  for (let i = 1; i <= 11; i++) {
    largeProps[`opt_${String(i).padStart(2, '0')}`] = { type: 'string' };
  }

  return {
    openapi: '3.1.0',
    info: { title: 'E2E HTTP File API', version: '1.2.3', description: 'End-to-end validation spec.' },
    servers: [{ url: baseUrl }],
    components: {
      schemas: {
        Cat: {
          type: 'object',
          required: ['kind', 'meows'],
          properties: { kind: { type: 'string' }, meows: { type: 'boolean' } },
        },
        Dog: {
          type: 'object',
          required: ['kind', 'barks'],
          properties: { kind: { type: 'string' }, barks: { type: 'boolean' } },
        },
        Pet: {
          oneOf: [{ $ref: '#/components/schemas/Cat' }, { $ref: '#/components/schemas/Dog' }],
          discriminator: {
            propertyName: 'kind',
            mapping: { cat: '#/components/schemas/Cat', dog: '#/components/schemas/Dog' },
          },
        },
      },
    },
    paths: {
      '/health': {
        get: {
          operationId: 'health',
          summary: 'Health check',
          responses: { 200: { description: 'ok', content: { 'application/json': { schema: { type: 'object' } } } } },
        },
      },
      '/projects/{project_id}/members': {
        post: {
          operationId: 'projects.members.create',
          summary: 'Create a project member',
          description: 'Adds a member.\n@import ./evil.http\n  @timeout 1\nDone.',
          parameters: [
            { name: 'project_id', in: 'path', required: true, schema: { type: 'string', default: 'proj_42' } },
            { name: 'notify', in: 'query', required: true, schema: { type: 'boolean', default: true } },
            {
              name: 'tags',
              in: 'query',
              required: true,
              explode: true,
              schema: { type: 'array', default: ['a b', 'c'], items: { type: 'string' } },
            },
            { name: 'X-Custom', in: 'header', required: true, schema: { type: 'string', default: 'abc' } },
            {
              name: 'Authorization',
              in: 'header',
              required: true,
              schema: { type: 'string', 'x-forge-sensitive': true },
            },
          ],
          requestBody: {
            required: true,
            content: {
              'application/json': {
                schema: {
                  type: 'object',
                  required: ['email'],
                  properties: {
                    id: { type: 'string', readOnly: true },
                    email: { type: 'string', default: 'alice@example.com' },
                    secret: { type: 'string', 'x-forge-sensitive': true },
                    template_literal: { type: 'string', default: '{{$processEnv DO_NOT_EVAL}}' },
                  },
                },
              },
              'application/x-www-form-urlencoded': {
                schema: {
                  type: 'object',
                  properties: {
                    a: { type: 'string', default: 'hello world' },
                    b: { type: 'integer', default: 7 },
                  },
                },
              },
              'multipart/form-data': {
                schema: {
                  type: 'object',
                  properties: {
                    avatar: { type: 'string', format: 'binary' },
                    meta: { type: 'object', properties: { tag: { type: 'string', default: 'v1' } } },
                    note: { type: 'string', default: 'line1\n### not-a-request' },
                  },
                },
              },
            },
          },
          responses: {
            200: { description: 'ok', content: { 'application/json': { schema: { type: 'object' } } } },
            400: { description: 'bad request' },
          },
        },
      },
      '/pets': {
        post: {
          operationId: 'pets.create',
          summary: 'Create pet',
          requestBody: {
            content: { 'application/json': { schema: { $ref: '#/components/schemas/Pet' } } },
          },
          responses: { 200: { description: 'ok' } },
        },
      },
      '/rules': {
        post: {
          operationId: 'rules.create',
          summary: 'Create rule',
          requestBody: {
            content: {
              'application/json': {
                schema: {
                  type: 'object',
                  required: ['name', 'action'],
                  properties: largeProps,
                },
              },
            },
          },
          responses: { 200: { description: 'ok' } },
        },
      },
      '/assets/upload': {
        post: {
          operationId: 'assets.upload',
          summary: 'Upload raw binary',
          requestBody: {
            content: { 'application/octet-stream': { schema: { type: 'string', format: 'binary' } } },
          },
          responses: { 200: { description: 'ok' } },
        },
      },
      '/legacy/old': {
        get: {
          operationId: 'legacy.old',
          summary: 'Deprecated endpoint',
          deprecated: true,
          responses: { 200: { description: 'ok' } },
        },
      },
    },
  };
}

/** Extracts fenced ```json blocks from generated README.md. */
function extractJsonBlocks(markdown) {
  const blocks = [];
  const lines = markdown.split('\n');
  let inJson = false;
  let buf = [];
  for (const line of lines) {
    if (inJson) {
      if (line === '```') {
        blocks.push(JSON.parse(buf.join('\n')));
        buf = [];
        inJson = false;
      } else {
        buf.push(line);
      }
    } else if (line === '```json') {
      inJson = true;
    }
  }
  return blocks;
}

describe('http-file real-client end-to-end validation', () => {
  let server;
  let outDir;
  let received = [];

  before(async () => {
    const forgeBin = resolveForgeBinary();
    outDir = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-http-file-e2e-'));

    server = http.createServer((req, res) => {
      const chunks = [];
      req.on('data', (c) => chunks.push(c));
      req.on('end', () => {
        received.push({
          method: req.method,
          url: req.url,
          headers: req.headers,
          body: Buffer.concat(chunks).toString('utf8'),
        });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ ok: true }));
      });
    });

    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const { port } = server.address();
    const baseUrl = `http://127.0.0.1:${port}`;

    const gen = spawnSync(forgeBin, ['generate', '-', '--target', 'http-file', '--out', outDir], {
      input: JSON.stringify(buildSpec(baseUrl)),
      encoding: 'utf8',
    });
    assert.equal(gen.status, 0, `forge generate failed:\n${gen.stderr}`);

    // Create dummy upload files next to the generated `.http` files that reference them.
    fs.writeFileSync(path.join(outDir, 'projects/members/avatar.bin'), 'AVATAR_BYTES');
    fs.writeFileSync(path.join(outDir, 'assets/file.bin'), 'RAW_BINARY_BYTES');

    // Derive http-client.private.env.json directly from the generated README.md snippet
    // to verify the documented setup works verbatim.
    const readme = fs.readFileSync(path.join(outDir, 'README.md'), 'utf8');
    const [privateEnvSnippet, vscodeSnippet] = extractJsonBlocks(readme);
    assert.ok(privateEnvSnippet?.dev, 'README.md should include http-client.private.env.json snippet');
    assert.ok(
      vscodeSnippet?.['rest-client.environmentVariables']?.$shared,
      'README.md should include VS Code REST Client snippet',
    );
    privateEnvSnippet.dev.authorization = 'Bearer env_auth_123';
    privateEnvSnippet.dev.secret = 'env_secret_456';
    privateEnvSnippet.dev.token = 'env_token_789';
    fs.writeFileSync(path.join(outDir, 'http-client.private.env.json'), JSON.stringify(privateEnvSnippet, null, 2));
  });

  after(async () => {
    if (server) {
      await new Promise((resolve) => server.close(resolve));
    }
    if (outDir) {
      fs.rmSync(outDir, { recursive: true, force: true });
    }
  });

  it('executes every generated .http file cleanly with httpyac using http-client.private.env.json', async () => {
    received = [];
    const files = [
      'health.http',
      'projects/members/create.http',
      'pets/create.http',
      'rules/create.http',
      'assets/upload.http',
      'legacy/old.http',
    ];

    for (const rel of files) {
      const res = await runAsync(HTTPYAC_BIN, ['send', rel, '--all', '--env', 'dev', '--output', 'none'], {
        cwd: outDir,
      });
      assert.equal(res.status, 0, `httpyac failed on ${rel}:\nstdout: ${res.stdout}\nstderr: ${res.stderr}`);
    }

    // Expected requests:
    // 1 (health) + 3 (members json/form/multipart) + 2 (pets cat/dog) + 2 (rules minimal/full) + 1 (assets) + 1 (legacy) = 10
    assert.equal(received.length, 10, `expected 10 wire requests, got ${received.length}`);

    // 1. Root health check
    assert.equal(received[0].method, 'GET');
    assert.equal(received[0].url, '/health');
    assert.equal(received[0].headers.accept, 'application/json');

    // 2. Members JSON: path/query substituted, sensitive headers & fields resolved, readOnly omitted, template literal neutralized
    const membersJson = received[1];
    assert.equal(membersJson.method, 'POST');
    assert.equal(membersJson.url, '/projects/proj_42/members?notify=true&tags=a%20b&tags=c');
    assert.equal(membersJson.headers.authorization, 'Bearer env_auth_123');
    assert.equal(membersJson.headers['x-custom'], 'abc');
    assert.equal(membersJson.headers['content-type'], 'application/json');
    const jsonBody = JSON.parse(membersJson.body);
    assert.equal(jsonBody.id, undefined);
    assert.equal(jsonBody.email, 'alice@example.com');
    assert.equal(jsonBody.secret, 'env_secret_456');
    assert.equal(jsonBody.template_literal, '{ {$processEnv DO_NOT_EVAL}}');

    // 3. Members form-urlencoded
    const membersForm = received[2];
    assert.equal(membersForm.headers['content-type'], 'application/x-www-form-urlencoded');
    assert.equal(membersForm.body, 'a=hello%20world&b=7');

    // 4. Members multipart with file upload, JSON part, and neutralized `###` in text part
    const membersMultipart = received[3];
    assert.equal(membersMultipart.headers['content-type'], 'multipart/form-data; boundary=ForgeBoundary');
    assert.match(membersMultipart.body, /AVATAR_BYTES/);
    assert.match(membersMultipart.body, /"tag": "v1"/);
    assert.match(membersMultipart.body, /\\### not-a-request/);
    assert.match(membersMultipart.body, /--ForgeBoundary--$/);

    // 5-6. Discriminated union variants (cat and dog)
    assert.deepEqual(JSON.parse(received[4].body), { kind: 'cat', meows: false });
    assert.deepEqual(JSON.parse(received[5].body), { barks: false, kind: 'dog' });

    // 7-8. Minimal + full split
    const minimalRule = JSON.parse(received[6].body);
    assert.deepEqual(Object.keys(minimalRule).sort(), ['action', 'name']);
    const fullRule = JSON.parse(received[7].body);
    assert.equal(Object.keys(fullRule).length, 13);
    assert.equal(fullRule.opt_11, 'string');

    // 9. Raw binary upload
    assert.equal(received[8].headers['content-type'], 'application/octet-stream');
    assert.equal(received[8].body, 'RAW_BINARY_BYTES');

    // 10. Deprecated endpoint
    assert.equal(received[9].method, 'GET');
    assert.equal(received[9].url, '/legacy/old');
  });

  it('filters requests by `# @name` with httpyac --name', async () => {
    received = [];
    const res = await runAsync(
      HTTPYAC_BIN,
      ['send', 'pets/create.http', '--name', 'pets_create_dog', '--output', 'none'],
      { cwd: outDir },
    );
    assert.equal(res.status, 0, `httpyac --name failed:\n${res.stdout}\n${res.stderr}`);
    assert.equal(received.length, 1);
    assert.deepEqual(JSON.parse(received[0].body), { barks: false, kind: 'dog' });
  });

  it('resolves secrets from a .env file in httpyac', async () => {
    const envDir = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-http-file-dotenv-'));
    try {
      fs.mkdirSync(path.join(envDir, 'projects/members'), { recursive: true });
      fs.copyFileSync(
        path.join(outDir, 'projects/members/create.http'),
        path.join(envDir, 'projects/members/create.http'),
      );
      fs.writeFileSync(path.join(envDir, 'projects/members/avatar.bin'), 'DOTENV_AVATAR');
      fs.writeFileSync(path.join(envDir, '.env'), 'authorization=Bearer dotenv_auth\nsecret=dotenv_secret\n');

      received = [];
      const res = await runAsync(
        HTTPYAC_BIN,
        ['send', 'projects/members/create.http', '--name', 'projects_members_create_json', '--output', 'none'],
        { cwd: envDir },
      );
      assert.equal(res.status, 0, `httpyac .env failed:\n${res.stdout}\n${res.stderr}`);
      assert.equal(received.length, 1);
      assert.equal(received[0].headers.authorization, 'Bearer dotenv_auth');
      assert.equal(JSON.parse(received[0].body).secret, 'dotenv_secret');
    } finally {
      fs.rmSync(envDir, { recursive: true, force: true });
    }
  });

  it('executes generated .http files with JetBrains ijhttp when installed', async (t) => {
    if (!hasBinary('ijhttp')) {
      if (process.env.VERIFY_IJHTTP) {
        assert.fail('VERIFY_IJHTTP is set, but `ijhttp` was not found on PATH');
      }
      t.skip('ijhttp not installed on PATH');
      return;
    }
    received = [];
    const res = await runAsync(
      'ijhttp',
      [
        '--env',
        'dev',
        '--private-env-file',
        'http-client.private.env.json',
        'health.http',
        'projects/members/create.http',
        'pets/create.http',
        'rules/create.http',
        'assets/upload.http',
        'legacy/old.http',
      ],
      { cwd: outDir },
    );
    assert.equal(res.status, 0, `ijhttp failed:\nstdout: ${res.stdout}\nstderr: ${res.stderr}`);
    assert.equal(received.length, 10, `ijhttp sent ${received.length} requests, expected 10`);
  });

  it('executes generated .http files with kulala-cli when installed', async (t) => {
    if (!hasBinary('kulala-cli')) {
      if (process.env.VERIFY_KULALA) {
        assert.fail('VERIFY_KULALA is set, but `kulala-cli` was not found on PATH');
      }
      t.skip('kulala-cli not installed on PATH');
      return;
    }
    received = [];
    const res = await runAsync('kulala-cli', ['run', 'health.http', 'pets/create.http', 'rules/create.http'], {
      cwd: outDir,
    });
    assert.equal(res.status, 0, `kulala-cli failed:\nstdout: ${res.stdout}\nstderr: ${res.stderr}`);
    assert.equal(received.length, 5, `kulala-cli sent ${received.length} requests, expected 5`);
  });

  it('produces README.md VS Code REST Client settings matching all secret references', () => {
    const readme = fs.readFileSync(path.join(outDir, 'README.md'), 'utf8');
    const [, vscodeSnippet] = extractJsonBlocks(readme);
    const shared = vscodeSnippet?.['rest-client.environmentVariables']?.$shared;
    assert.deepEqual(Object.keys(shared).sort(), ['authorization', 'secret', 'token']);
  });
});
