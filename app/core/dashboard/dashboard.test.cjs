const assert = require('node:assert/strict')
const { readFileSync } = require('node:fs')
const { test } = require('node:test')
const vm = require('node:vm')

const html = readFileSync(`${__dirname}/dashboard.html`, 'utf8')
const scripts = [...html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .map((match) => match[1])
  .filter((script) => script.trim() !== '')
assert.equal(scripts.length, 1, 'expected one inline dashboard script')

// Keep the real initialization and handlers, omitting only the network poll and its timer.
const polling = '\npoll()\nsetInterval(poll, POLL_MS)\n'
assert.ok(scripts[0].endsWith(polling), 'dashboard polling startup changed')
const source = scripts[0].slice(0, -polling.length)

class Target {
  listeners = new Map()

  addEventListener(type, listener, options = {}) {
    const entries = this.listeners.get(type) ?? []
    entries.push({ listener, once: options.once })
    this.listeners.set(type, entries)
  }

  removeEventListener(type, listener) {
    this.listeners.set(type, (this.listeners.get(type) ?? []).filter((entry) => entry.listener !== listener))
  }

  dispatch(type, properties = {}) {
    for (const { listener, once } of [...(this.listeners.get(type) ?? [])]) {
      listener({ currentTarget: this, ...properties })
      if (once) this.removeEventListener(type, listener)
    }
  }
}

// Returns the `.row.card` rows of `html`, each with its decoded `data-dir`.
function cardRows(html) {
  return [...html.matchAll(/class="row card[^"]*" data-dir="([^"]*)"/g)].map((match) => ({
    dataset: { dir: match[1].replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code)) },
    classList: { toggle: () => {} },
    scrollIntoView: () => {},
  }))
}

function dashboard({ script = source, pageWidth = 1200, pageLeft = 0, splitterWidth = 6, fetcher, storage = new Map() } = {}) {
  const elements = new Map()
  const calls = []
  const geometry = { pageWidth, pageLeft, splitterWidth }

  const element = (id) => {
    const properties = new Map()
    const classes = new Set()
    const el = Object.assign(new Target(), {
      id,
      style: {
        setProperty: (name, value) => properties.set(name, value),
        getPropertyValue: (name) => properties.get(name) ?? '',
      },
      classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name) },
      setPointerCapture: () => {},
      querySelector: () => null,
      scrollTop: 0,
      firstElementChild: null,
      innerHTML: '',
      getBoundingClientRect: () => ({
        width: id === 'splitter' ? geometry.splitterWidth : geometry.pageWidth,
        left: geometry.pageLeft,
      }),
    })
    elements.set(id, el)
    return el
  }

  for (const id of ['page', 'status', 'problems', 'font-down', 'font-up', 'font-value']) element(id)

  let pageHtml = ''
  elements.get('page').querySelectorAll = () => cardRows(pageHtml)
  Object.defineProperty(elements.get('page'), 'innerHTML', {
    get: () => pageHtml,
    set(value) {
      pageHtml = value
      if (!value.includes('class="session-page"')) {
        this.firstElementChild = element('list')
        return
      }

      this.firstElementChild = element('session-page')
      for (const id of ['metro-pane', 'session-card', 'metro', 'splitter', 'viewer-head', 'view']) {
        element(id).parentElement = this.firstElementChild
      }
    },
  })

  const document = Object.assign(new Target(), {
    getElementById: (id) => elements.get(id) ?? null,
    documentElement: { style: {} },
  })
  const window = new Target()
  const context = vm.createContext({
    document,
    window,
    location: { hash: '' },
    localStorage: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, String(value)),
    },
    fetch: async (url, options) => {
      calls.push(url)
      if (!fetcher) throw new Error(`unexpected fetch: ${url}`)
      return fetcher(url, options)
    },
  })
  const run = (code) => vm.runInContext(code, context, { filename: 'dashboard.html' })
  run(script)
  return { run, context, elements, window, calls, geometry }
}

const session = { org: 'org', repo: 'repo', id: 'session', dir: 'api/file/org/repo/session/', flow: 'build', stage: 'implement', round: 0 }
const filesUrl = 'api/files/org/repo/session'
const response = (text, ok = true) => ({ ok, status: ok ? 200 : 500, text: async () => text })
const filesReply = (files) => response(JSON.stringify({ files }))
const sessionsReply = (sessions) => response(JSON.stringify({ sessions }))
const plain = (value) => JSON.parse(JSON.stringify(value))

function modelGate(app) {
  const failures = Array.from(app.run('metroSelfCheck()'))
  assert.equal(failures.length, 0, `metro self-check failed: ${failures.join(', ')}`)
}

test('the actual dashboard model passes every self-check', () => {
  const app = dashboard()
  modelGate(app)
  assert.equal(app.calls.length, 0)
})

test('the model gate rejects integerOrNull returning zero', () => {
  const original = 'function integerOrNull(value) {\n  return Number.isInteger(value) ? value : null\n}'
  assert.ok(source.includes(original), 'integerOrNull mutation target changed')
  const script = source.replace(original, 'function integerOrNull(value) {\n  return 0\n}')
  assert.throws(() => modelGate(dashboard({ script })), /metro self-check failed:.*bad-types/)
})

test('rendering an old session lists files and never fetches an absent run log', async () => {
  const app = dashboard({ fetcher: () => filesReply(['session.json']) })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [filesUrl])
})

test('a listed run log is parsed after listing and tolerates a truncated line', async () => {
  const app = dashboard({
    fetcher: (url) => url.endsWith('runs.jsonl')
      ? response('{"stop":"spec","round":0,"agent":"writer","tokens":5}\n{"stop":"pl')
      : filesReply(['runs.jsonl']),
  })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [filesUrl, `${session.dir}runs.jsonl`])
  assert.match(app.elements.get('session-card').innerHTML, /5 agent tokens/)
  const runs = await app.run('loadRuns("a space/", ["runs.jsonl"])')
  assert.equal(runs.length, 1)
  assert.equal(runs[0].tokens, 5)
  assert.equal(app.calls.at(-1), 'a%20space/runs.jsonl')
})

test('run logs remain optional when fetching fails', async () => {
  for (const fetcher of [() => response('', false), () => { throw new Error('disconnected') }]) {
    const app = dashboard({ fetcher })
    assert.deepEqual(plain(await app.run('loadRuns("session/", ["runs.jsonl"])')), [])
    assert.deepEqual(app.calls, ['session/runs.jsonl'])
    assert.deepEqual(plain(await app.run('loadRuns("session/", [])')), [])
    assert.equal(app.calls.length, 1)
  }
})

test('splitter clamps dragging, resizing and reopening to the actual page and splitter widths', async () => {
  const app = dashboard({ pageWidth: 1200, pageLeft: 100, splitterWidth: 10, fetcher: () => response('') })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  const splitter = app.elements.get('splitter')
  const left = () => Number.parseInt(app.elements.get('session-page').style.getPropertyValue('--left'))
  let prevented = false
  splitter.dispatch('pointerdown', { pointerId: 1, preventDefault: () => { prevented = true } })
  splitter.dispatch('pointermove', { clientX: 2000 })
  assert.ok(prevented)
  assert.equal(left(), 890)
  splitter.dispatch('pointermove', { clientX: 600 })
  assert.equal(left(), 500, 'drag accounts for the page offset')
  splitter.dispatch('pointermove', { clientX: 0 })
  assert.equal(left(), 320)
  splitter.dispatch('pointermove', { clientX: 2000 })
  splitter.dispatch('lostpointercapture')
  splitter.dispatch('pointermove', { clientX: 0 })
  assert.equal(left(), 890, 'drag stops when pointer capture is lost')

  app.geometry.pageWidth = 900
  app.window.dispatch('resize')
  assert.equal(left(), 590)
  app.geometry.pageWidth = 600
  app.window.dispatch('resize')
  assert.equal(left(), 290, 'viewer reservation wins when both column minima cannot fit')

  app.geometry.splitterWidth = 0
  app.geometry.pageWidth = 400
  app.window.dispatch('resize')
  assert.equal(left(), 290, 'mobile layout keeps the saved desktop width')
  assert.match(html, /@media \(max-width: 800px\)\s*\{\s*\.session-page \{ grid-template-columns: minmax\(0, 1fr\)/)

  app.elements.get('page').innerHTML = '<div>list</div>'
  app.geometry.pageWidth = 850
  app.geometry.splitterWidth = 10
  app.window.dispatch('resize')
  app.run('leftWidth = 1200')
  await app.run('renderSessionPage(session, null)')
  assert.equal(left(), 540)
  assert.ok(app.geometry.pageWidth - left() - app.geometry.splitterWidth >= 300)
})

test('window resizing before opening a session is safe', () => {
  dashboard().window.dispatch('resize')
})

test('Request timing is hidden in every flow, including legacy zero and derived durations', () => {
  const app = dashboard()
  for (const flow of ['build', 'review', 'vibe']) {
    for (const durationMs of [0, 5000, null]) {
      app.context.session = { ...session, flow }
      app.context.entry = { stop: 'request', round: 0, durationMs, started: '2026-10-22T10:00:00Z', ended: '2026-10-22T10:01:00Z' }
      const card = app.run('stopCardHtml(buildStops(session, ["request.md"], parseRuns(JSON.stringify(entry)))[0], null, false)')
      assert.doesNotMatch(card, /class="when"/)
    }
  }
  const card = app.run('stopCardHtml(withRuns(newStop("implement", 1, null, []), parseRuns(\'{"stop":"implement","round":1,"durationMs":5000}\')), null, false)')
  assert.match(card, /class="when">5s/)
})

test('session facts include stage, last round, deferred gaps and only recorded agent tokens', () => {
  const app = dashboard()
  app.context.session = { ...session, stage: 'done', round: 2, lastRound: true, counts: { deferred: 5, issued: 2 } }
  app.context.entries = [
    { stop: 'report', round: 1, agent: 'main', tokens: 9000 },
    { stop: 'implement', round: 1, agent: 'worker', tokens: 10 },
    { stop: 'review', round: 1, agent: 'reviewer', tokens: 5 },
    { stop: 'review', round: 1, agent: null, tokens: 1000 },
    { stop: 'triage', round: 1, agent: 'triager', tokens: null },
  ]
  const card = app.run('sessionCardHtml(session, parseRuns(entries.map(JSON.stringify).join("\\n")))')
  for (const fact of ['done', 'round 2', 'last round', '3 deferred without issue', '15 agent tokens']) {
    assert.ok(card.includes(`>${fact}</span>`), `missing session fact: ${fact}`)
  }
  app.context.session = { ...session, stage: 'blocked', blocked: { stage: 'implement' } }
  assert.match(app.run('sessionCardHtml(session, [])'), />blocked at implement<\/span>/)
  assert.doesNotMatch(app.run('sessionCardHtml(session, parseRuns(JSON.stringify(entries[0])))'), /agent tokens/)
  assert.doesNotMatch(app.run('sessionCardHtml(session, parseRuns(JSON.stringify(entries[4])))'), /agent tokens/)
  assert.match(app.run('sessionCardHtml(session, parseRuns(\'{"stop":"spec","round":0,"agent":"worker","tokens":0}\'))'), /0 agent tokens/)
})

test('blocked Implement keeps existing tasks as a link and session.json as its main file', () => {
  const app = dashboard()
  for (const flow of ['build', 'vibe']) {
    app.context.session = { ...session, flow, stage: 'blocked', blocked: { stage: 'implement', reason: 'needs input' } }
    const stop = app.run('buildStops(session, ["tasks.md"], []).find((stop) => stop.state === "blocked")')
    assert.equal(stop.file, 'session.json')
    assert.equal(stop.note, 'needs input')
    assert.deepEqual(plain(stop.links), [{ label: 'tasks', path: 'tasks.md' }])
    assert.deepEqual(plain(app.run('buildStops(session, [], []).find((stop) => stop.state === "blocked").links')), [])
  }
})

test('review rounds close on successful comments without submit.md and retain submit.md compatibility', () => {
  const app = dashboard()
  app.context.session = { flow: 'review', stage: 'verify', round: 1 }
  assert.equal(app.run('openRound(session, [], parseRuns(\'{"stop":"comment","round":1,"outcome":"done"}\'))'), 2)
  assert.equal(app.run('openRound(session, [], parseRuns(\'{"stop":"comment","round":1,"outcome":"failed"}\'))'), 1)
  assert.equal(app.run('openRound(session, ["round-1/submit.md"], [])'), 2)
})

test('finalize skips the following Implement while the next-round outcome retains it', () => {
  const app = dashboard()
  app.context.session = { ...session, stage: 'verify', round: 1 }
  assert.equal(app.run('buildStops(session, ["round-1/triage.md"], [], { 1: "finalize" }).some((stop) => stop.kind === "implement" && stop.round === 2)'), false)
  assert.equal(app.run('buildStops(session, ["round-1/triage.md"], [], { 1: "next round" }).some((stop) => stop.kind === "implement" && stop.round === 2)'), true)
})

test('rendering a session reads each round triage outcome and skips Implement after finalize', async () => {
  const pages = {
    [filesUrl]: JSON.stringify({ files: ['round-1/triage.md'] }),
    [`${session.dir}round-1/triage.md`]: '# Triage: round 1\n\n- F1: dismiss → D1\n\noutcome: finalize\n',
  }
  const app = dashboard({ fetcher: (url) => response(pages[url] ?? '', Object.hasOwn(pages, url)) })
  app.context.session = { ...session, stage: 'verify', round: 1 }
  await app.run('renderSessionPage(session, null)')

  const stops = app.run('sessionView.stops')
  assert.equal(stops.some((stop) => stop.kind === 'implement' && stop.round === 2), false, 'Implement shown after finalize')
  assert.equal(stops.find((stop) => stop.kind === 'review' && stop.round === 2)?.state, 'current')
  assert.ok(app.calls.includes(`${session.dir}round-1/triage.md`), 'triage outcome was not fetched')
})

test('parallel Review time spans all runs and falls back to durations when any timestamp is missing or invalid', () => {
  const app = dashboard()
  app.context.entries = [
    { stop: 'review', round: 1, started: '2026-10-22T10:00:00Z', ended: '2026-10-22T10:05:00Z', durationMs: null },
    { stop: 'review', round: 1, started: '2026-10-22T10:01:00Z', ended: '2026-10-22T10:09:00Z', durationMs: 480000 },
    { stop: 'report', round: 1, started: '2026-10-22T10:09:00Z', ended: '2026-10-22T10:09:45Z', durationMs: 45000 },
  ]
  const review = () => app.run('reviewStop(1, [], parseRuns(entries.map(JSON.stringify).join("\\n")), ["review", "report"])')
  assert.equal(review().durationMs, 585000)
  assert.match(app.run('stopCardHtml(reviewStop(1, [], parseRuns(entries.map(JSON.stringify).join("\\n")), ["review", "report"]), null, false)'), /9m 45s/)
  for (const started of [null, 'invalid']) {
    app.context.entries[1].started = started
    assert.equal(review().durationMs, 825000)
  }
})

test('loads sessions from the api in one call', async () => {
  const entries = [
    { org: 'a', repo: 'a b%c', session: 's1', data: { repo: 'other', id: 'other', stage: 'implement' } },
    { org: 'b', repo: 'repo', session: 's2', data: { stage: 'spec' } },
  ]
  const app = dashboard({ fetcher: (url) => (url === 'api/sessions' ? sessionsReply(entries) : filesReply([])) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(app.calls, ['api/sessions'])
  assert.deepEqual(problems, [])
  assert.deepEqual(
    sessions.map(({ org, repo, id, dir, stage }) => ({ org, repo, id, dir, stage })),
    [
      { org: 'a', repo: 'a b%c', id: 's1', dir: 'api/file/a/a b%c/s1/', stage: 'implement' },
      { org: 'b', repo: 'repo', id: 's2', dir: 'api/file/b/repo/s2/', stage: 'spec' },
    ],
  )

  app.context.loaded = sessions
  await app.run('loadRuns(loaded[0].dir, ["runs.jsonl"])')
  await app.run('listFiles(loaded[0])')
  assert.deepEqual(app.calls.slice(1), ['api/file/a/a%20b%25c/s1/runs.jsonl', 'api/files/a/a%20b%25c/s1'])
})

test('lists a broken session as a problem', async () => {
  const entries = [
    { org: 'a', repo: 'repo', session: 'broken', error: 'expected value at line 1 column 1' },
    { org: 'a', repo: 'repo', session: 'fine', data: { flow: 'build', stage: 'spec' } },
    { org: 'b', repo: 'repo', session: 'odd', data: { flow: 'build', stage: 'nope' } },
  ]
  const app = dashboard({ fetcher: () => sessionsReply(entries) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(sessions.map((s) => s.id), ['fine', 'odd'])
  assert.deepEqual(problems, ['a/repo/broken: expected value at line 1 column 1'])

  await app.run('poll()')
  const shown = app.elements.get('problems').innerHTML
  assert.ok(shown.includes('<div>a/repo/broken: expected value at line 1 column 1</div>'), shown)
  assert.ok(shown.includes('<div>b/repo/odd: unknown stage &#34;nope&#34;</div>'), shown)
  assert.equal(app.elements.get('status').textContent, '2 sessions')
})

test('parses routes with the org', () => {
  const app = dashboard()
  assert.equal(app.run('parseRoute("#/")'), null)
  assert.deepEqual(plain(app.run('parseRoute("#/org/a%20b%25c/s1")')), { org: 'org', repo: 'a b%c', id: 's1', path: null })
  assert.deepEqual(
    plain(app.run('parseRoute("#/org/repo/s1/round-1/report.md")')),
    { org: 'org', repo: 'repo', id: 's1', path: 'round-1/report.md' },
  )

  app.context.session = { org: 'org', repo: 'a b%c', id: 's1' }
  assert.equal(app.run('sessionHref(session)'), '#/org/a%20b%25c/s1')
  assert.equal(app.run('fileHref(session, "round-1/report.md")'), '#/org/a%20b%25c/s1/round-1/report.md')
  assert.equal(app.run('sessionHref(parseRoute(sessionHref(session)))'), '#/org/a%20b%25c/s1')
})

test('groups sessions by org and filters them', () => {
  const at = (org, id, updated) => ({ ...session, org, id, dir: `api/file/${org}/repo/${id}/`, updated })
  const sessions = [at('b', 'b1', '2026-10-22T10:00:00Z'), at('a', 'a1', '2026-10-22T09:00:00Z'), at('a', 'a2', '2026-10-22T11:00:00Z')]
  const [b1, a1, a2] = sessions.map((s) => s.dir)

  // Returns the org headings and the session dirs of the list, in page order.
  const list = (app) => [...app.elements.get('page').innerHTML.matchAll(/<h2 class="org">([^<]*)<\/h2>|data-dir="([^"]*)"/g)]
    .map((match) => match[1] ?? match[2])

  // Returns the labels of the org filter, with `*` after the pressed one.
  const filter = (app) => [...app.elements.get('page').innerHTML.matchAll(/data-org="[^"]*" aria-pressed="(true|false)">([^<]*)</g)]
    .map((match) => `${match[2]}${match[1] === 'true' ? '*' : ''}`)

  const choose = (app, org) => app.elements.get('list').dispatch('click', {
    target: { closest: (selector) => (selector === 'button[data-org]' ? { dataset: { org } } : null) },
  })

  const storage = new Map()
  const app = dashboard({ storage })
  app.context.sessions = sessions
  app.run('state.sessions = sessions; renderList()')
  assert.deepEqual(list(app), ['a', a2, a1, 'b', b1])
  assert.deepEqual(filter(app), ['All*', 'a', 'b'])

  app.run('listKey("ArrowDown")')
  assert.equal(app.run('selectedDir'), a2)

  choose(app, 'b')
  assert.equal(storage.get('jam-board-org'), 'b')
  assert.deepEqual(list(app), ['b', b1])
  assert.deepEqual(filter(app), ['All', 'a', 'b*'])
  assert.equal(app.run('selectedDir'), null, 'the filter clears a hidden selection')

  app.run('listKey("ArrowDown"); listKey("ArrowDown")')
  assert.equal(app.run('selectedDir'), b1)
  app.run('listKey("ArrowUp")')
  assert.equal(app.run('selectedDir'), b1)
  assert.equal(app.run(`selectedDir = ${JSON.stringify(a2)}; listKey("Enter")`), false, 'a hidden session takes no key')

  const restored = dashboard({ storage })
  restored.context.sessions = sessions
  restored.run('state.sessions = sessions; renderList()')
  assert.deepEqual(list(restored), ['b', b1])

  restored.run('state.sessions = state.sessions.filter((s) => s.org === "a"); renderList()')
  assert.deepEqual(list(restored), ['a', a2, a1], 'a stored org without sessions shows every org')
  assert.deepEqual(filter(restored), ['All*', 'a'])

  choose(app, '')
  assert.equal(storage.get('jam-board-org'), '')
  assert.deepEqual(list(app), ['a', a2, a1, 'b', b1])

  app.run('state.sessions = []; renderList()')
  assert.deepEqual(filter(app), [])
  assert.match(app.elements.get('page').innerHTML, /No sessions yet\./)
})

test('archive and open post to the api routes with the org', async () => {
  const posts = []
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') posts.push(url)
      return url === 'api/sessions' ? sessionsReply([]) : response('')
    },
  })
  app.context.messages = []
  app.context.session = { ...session, org: 'my org', repo: 'a b%c', id: 's1' }
  app.run('ask = async (message) => { messages.push(message); return true }')

  await app.run('archiveSession(session)')
  await app.run('openTool(session, "zed")')
  await new Promise(setImmediate)
  assert.deepEqual(posts, ['api/archive/my%20org/a%20b%25c/s1', 'api/open/zed/my%20org/a%20b%25c/s1'])
  assert.match(app.context.messages[0], /^Archive my org\/a b%c\/s1\?/)
  assert.equal(app.context.location.hash, '#/')

  const failing = dashboard({ fetcher: () => response('zed is missing', false) })
  failing.context.messages = []
  failing.context.session = app.context.session
  failing.run('tell = async (message) => { messages.push(message) }')
  await failing.run('openTool(session, "zed")')
  assert.deepEqual(failing.context.messages, ['Could not open my org/a b%c/s1 in Zed: zed is missing'])
})
