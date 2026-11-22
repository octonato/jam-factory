const assert = require('node:assert/strict')
const { readFileSync } = require('node:fs')
const { test } = require('node:test')
const vm = require('node:vm')

const html = readFileSync(`${__dirname}/dashboard.html`, 'utf8')
const scripts = [...html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .map((match) => match[1])
  .filter((script) => script.trim() !== '')
assert.equal(scripts.length, 1, 'expected one inline dashboard script')

// Keep the real initialization and handlers, omitting only the version load, the network poll and its timer.
const polling = '\nloadVersion()\npoll()\nsetInterval(poll, POLL_MS)\n'
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

function dashboard({ script = source, pageWidth = 1200, pageLeft = 0, pageTop = 0, pageHeight = 800, splitterWidth = 6, fetcher, storage = new Map(), search = '', hash = '', tauri } = {}) {
  const elements = new Map()
  const calls = []
  const geometry = { pageWidth, pageLeft, pageTop, pageHeight, splitterWidth }

  // Moves each of `nodes` from its parent to the end of the children of `parent`.
  const appendTo = (parent, nodes) => {
    for (const node of nodes) {
      node.remove?.()
      node.parentElement = parent
      parent.children = [...parent.children, node]
    }
  }

  const element = (id) => {
    const properties = new Map()
    const classes = new Set()
    let html = ''
    const el = Object.assign(new Target(), {
      id,
      style: {
        setProperty: (name, value) => properties.set(name, value),
        getPropertyValue: (name) => properties.get(name) ?? '',
      },
      classList: {
        add: (name) => classes.add(name),
        remove: (name) => classes.delete(name),
        toggle: (name, force = !classes.has(name)) => {
          if (force) classes.add(name)
          else classes.delete(name)
          return force
        },
        contains: (name) => classes.has(name),
      },
      setPointerCapture: () => {},
      value: '',
      selected: false,
      focused: false,
      focus: () => { el.focused = true },
      blur: () => { el.focused = false },
      select: () => { el.selected = true },
      querySelector: () => null,
      querySelectorAll: () => [],
      closest: (selector) => (selector.split(',').some((part) => part.trim() === `#${id}`) ? el : el.parentElement?.closest(selector) ?? null),
      replaceChildren: (...nodes) => {
        html = ''
        el.children = nodes
        for (const node of nodes) node.parentElement = el
      },
      append: (...nodes) => appendTo(el, nodes),
      children: [],
      scrollTop: 0,
      firstElementChild: null,
      getBoundingClientRect: () => ({
        width: id === 'splitter' ? geometry.splitterWidth : geometry.pageWidth,
        height: geometry.pageHeight,
        left: geometry.pageLeft,
        top: geometry.pageTop,
      }),
    })
    Object.defineProperty(el, 'innerHTML', {
      configurable: true,
      get: () => html,
      set: (value) => {
        html = value
        el.children = []
        if (id === 'term-tabs') {
          elements.delete('term-rename')
          const rename = value.match(/ id="term-rename" value="([^"]*)"/)
          if (rename !== null) {
            const input = element('term-rename')
            input.parentElement = el
            input.value = rename[1].replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code))
          }
        }
      },
    })
    elements.set(id, el)
    return el
  }

  // Returns a detached element of `tag` that takes the focus, holds children, and matches `tag` and the ids of its parents. A textarea turns CRLF and CR in its value into LF.
  const createElement = (tag) => {
    let value = ''
    const el = {
      tagName: tag.toUpperCase(),
      get value() { return value },
      set value(text) { value = tag === 'textarea' ? text.replace(/\r\n?/g, '\n') : text },
      focused: false,
      parentElement: null,
      children: [],
      focus: () => { el.focused = true },
      blur: () => { el.focused = false },
      replaceChildren: (...nodes) => {
        el.children = []
        appendTo(el, nodes)
      },
      append: (...nodes) => appendTo(el, nodes),
      remove: () => {
        if (el.parentElement) el.parentElement.children = el.parentElement.children.filter((child) => child !== el)
        el.parentElement = null
      },
      closest: (selector) => (selector.split(',').some((part) => part.trim() === tag) ? el : el.parentElement?.closest(selector) ?? null),
    }
    return el
  }

  for (const id of ['board-header', 'tabs', 'page', 'board-footer', 'version', 'term-store', 'status', 'problems', 'font-down', 'font-up', 'font-value', 'modal', 'modal-text', 'modal-input', 'modal-cancel', 'shortcuts', 'shortcuts-list', 'jump', 'jump-tabs', 'jump-input', 'jump-list']) element(id)
  for (const dialog of [elements.get('modal'), elements.get('shortcuts'), elements.get('jump')]) {
    dialog.showModal = () => { dialog.open = true }
    dialog.close = (value = '') => {
      dialog.returnValue = value
      dialog.open = false
      dialog.dispatch('close')
    }
  }

  let pageHtml = ''
  elements.get('page').querySelectorAll = () => cardRows(pageHtml)
  Object.defineProperty(elements.get('page'), 'innerHTML', {
    get: () => pageHtml,
    set(value) {
      pageHtml = value
      this.firstElementChild = element(/class="session-page[ "]/.test(value) ? 'session-page' : 'list')
      for (const [, id] of value.matchAll(/ id="([^"]+)"/g)) {
        element(id).parentElement = this.firstElementChild
      }
    },
  })

  const document = Object.assign(new Target(), {
    getElementById: (id) => elements.get(id) ?? null,
    createElement,
    documentElement: { style: { setProperty(name, value) { this[name] = value } } },
    visibilityState: 'visible',
    focused: true,
    hasFocus() { return this.focused },
  })
  const window = new Target()
  Object.defineProperty(window, 'innerHeight', { get: () => geometry.pageTop + geometry.pageHeight })
  if (tauri !== undefined) window.__TAURI__ = tauri
  const context = vm.createContext({
    document,
    window,
    location: { hash, search, replace(url) { this.hash = url } },
    URLSearchParams,
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

const session = { org: 'org', repo: 'repo', branch: 'main', id: 'session', dir: 'api/file/org/repo/main/session/', flow: 'build', stage: 'implement', round: 0 }
const filesUrl = 'api/files/org/repo/main/session'
const response = (text, ok = true, { status = ok ? 200 : 500, headers = {} } = {}) => ({
  ok,
  status,
  headers: new Headers(headers),
  text: async () => text,
})
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
  await app.run('renderWorktreePage("", session, null)')
  assert.deepEqual(app.calls, [filesUrl])
})

test('a listed run log is parsed after listing and tolerates a truncated line', async () => {
  const app = dashboard({
    fetcher: (url) => url.endsWith('runs.jsonl')
      ? response('{"stop":"spec","round":0,"agent":"writer","tokens":5}\n{"stop":"pl')
      : filesReply(['runs.jsonl']),
  })
  app.context.session = session
  await app.run('renderWorktreePage("", session, null)')
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
  await app.run('renderWorktreePage("", session, null)')
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
  assert.match(html, /@media \(max-width: 800px\)\s*\{\s*\.session-page \{ grid-template-columns: minmax\(0, 1fr\); grid-template-rows: auto auto auto minmax\(0, 55fr\) minmax\(0, 45fr\); \}/)

  app.elements.get('page').innerHTML = '<div>list</div>'
  app.geometry.pageWidth = 850
  app.geometry.splitterWidth = 10
  app.window.dispatch('resize')
  app.run('leftWidth = 1200')
  await app.run('renderWorktreePage("", session, null)')
  assert.equal(left(), 540)
  assert.ok(app.geometry.pageWidth - left() - app.geometry.splitterWidth >= 300)
})

test('window resizing before opening a session is safe', () => {
  dashboard().window.dispatch('resize')
})

test('revealing a stop scrolls its card into view without a header offset', async () => {
  const app = dashboard({ fetcher: () => filesReply(['session.json']) })
  app.context.session = session
  await app.run('renderWorktreePage("", session, null)')

  const card = { style: {}, scrolled: null, scrollIntoView(options) { this.scrolled = options } }
  app.elements.get('metro').querySelector = () => card
  app.run('revealStop(0)')

  assert.equal(card.style.scrollMarginTop, undefined)
  assert.deepEqual(plain(card.scrolled), { block: 'nearest' })
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
  const card = app.run('sessionStripHtml(session, parseRuns(entries.map(JSON.stringify).join("\\n")))')
  for (const fact of ['done', 'round 2', 'last round', '3 deferred without issue', '15 agent tokens']) {
    assert.ok(card.includes(`>${fact}</span>`), `missing session fact: ${fact}`)
  }
  app.context.session = { ...session, stage: 'blocked', blocked: { stage: 'implement' } }
  assert.match(app.run('sessionStripHtml(session, [])'), />blocked at implement<\/span>/)
  assert.doesNotMatch(app.run('sessionStripHtml(session, parseRuns(JSON.stringify(entries[0])))'), /agent tokens/)
  assert.doesNotMatch(app.run('sessionStripHtml(session, parseRuns(JSON.stringify(entries[4])))'), /agent tokens/)
  assert.match(app.run('sessionStripHtml(session, parseRuns(\'{"stop":"spec","round":0,"agent":"worker","tokens":0}\'))'), /0 agent tokens/)
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
  await app.run('renderWorktreePage("", session, null)')

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
    { org: 'a', repo: 'a b%c', branch: 'fix', session: 's1', category: 'walk', data: { repo: 'other', id: 'other', stage: 'implement' } },
    { org: 'b', repo: 'repo', branch: 'main', session: 's2', category: 'session', data: { stage: 'spec' } },
  ]
  const app = dashboard({ fetcher: (url) => (url === 'api/sessions' ? sessionsReply(entries) : filesReply([])) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(app.calls, ['api/sessions'])
  assert.deepEqual(problems, [])
  assert.deepEqual(
    sessions.map(({ org, repo, branch, id, category, dir, stage }) => ({ org, repo, branch, id, category, dir, stage })),
    [
      { org: 'a', repo: 'a b%c', branch: 'fix', id: 's1', category: 'walk', dir: 'api/file/a/a b%c/fix/s1/', stage: 'implement' },
      { org: 'b', repo: 'repo', branch: 'main', id: 's2', category: 'session', dir: 'api/file/b/repo/main/s2/', stage: 'spec' },
    ],
  )

  app.context.loaded = sessions
  await app.run('loadRuns(loaded[0].dir, ["runs.jsonl"])')
  await app.run('listFiles(loaded[0])')
  assert.deepEqual(app.calls.slice(1), ['api/file/a/a%20b%25c/fix/s1/runs.jsonl', 'api/files/a/a%20b%25c/fix/s1'])
})

test('lists a broken session as a problem', async () => {
  const entries = [
    { org: 'a', repo: 'repo', branch: 'main', session: 'broken', category: 'session', error: 'expected value at line 1 column 1' },
    { org: 'a', repo: 'repo', branch: 'main', session: 'fine', category: 'session', data: { flow: 'build', stage: 'spec' } },
    { org: 'b', repo: 'repo', branch: 'main', session: 'odd', category: 'session', data: { flow: 'build', stage: 'nope' } },
  ]
  const app = dashboard({ fetcher: () => sessionsReply(entries) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(sessions.map((s) => s.id), ['fine', 'odd'])
  assert.deepEqual(problems, ['a/repo/main/broken: expected value at line 1 column 1'])

  await app.run('poll()')
  const shown = app.elements.get('problems').innerHTML
  assert.ok(shown.includes('<div>a/repo/main/broken: expected value at line 1 column 1</div>'), shown)
  assert.ok(shown.includes('<div>b/repo/main/odd: unknown stage &#34;nope&#34;</div>'), shown)
  assert.equal(app.elements.get('status').textContent, '2 sessions')
})

test('missing xterm files are listed as problems', () => {
  const missing = Array.from(dashboard().run('missingScripts()'))
  assert.ok(missing.includes('xterm.js not loaded'), missing.join(', '))
  assert.ok(missing.includes('addon-fit.js not loaded'), missing.join(', '))
})

test('missing highlight files are listed as problems', () => {
  const missing = Array.from(dashboard().run('missingScripts()'))
  assert.ok(missing.includes('highlight.min.js not loaded'), missing.join(', '))
  assert.ok(missing.includes('diff2html-ui-base.min.js not loaded'), missing.join(', '))
})

test('parses fence info strings', () => {
  const app = dashboard()
  assert.deepEqual(plain(app.run("parseInfo('rust path=a/b.rs lines=2-9')")), { lang: 'rust', path: 'a/b.rs', lines: '2-9' })
  assert.deepEqual(plain(app.run("parseInfo('diff')")), { lang: 'diff', path: null, lines: null })
  assert.deepEqual(plain(app.run("parseInfo('')")), { lang: '', path: null, lines: null })
})

const markedScript = readFileSync(`${__dirname}/marked.min.js`, 'utf8')
const highlightScript = readFileSync(`${__dirname}/highlight.min.js`, 'utf8')
const highlightScalaScript = readFileSync(`${__dirname}/highlight-scala.min.js`, 'utf8')

// Returns a dashboard with the real marked and highlight.js, a DOMPurify stub that returns its input, and empty stubs for the other page scripts.
function markdownDashboard(options) {
  const app = dashboard(options)
  app.run(markedScript)
  app.run(highlightScript)
  app.run(highlightScalaScript)
  Object.assign(app.context, {
    DOMPurify: { sanitize: (html) => html },
    Diff2HtmlUI: class {},
    Terminal: class {},
    FitAddon: {},
  })
  return app
}

const diffText = [
  'diff --git a/f.txt b/f.txt',
  'index 1111111..2222222 100644',
  '--- a/f.txt',
  '+++ b/f.txt',
  '@@ -1 +1 @@',
  '-old',
  '+new',
].join('\n')
const diffFence = `## Step 1\n\n\`\`\`diff\n${diffText}\n\`\`\`\n`

test('a diff fence becomes a diff block with a toggle', () => {
  const app = markdownDashboard()
  app.context.text = diffFence
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="diff" data-diff="0"><div class="diff-bar">'), html)
  assert.ok(html.includes('<button type="button" data-layout="line-by-line" aria-pressed="true">Unified</button>'), html)
  assert.ok(html.includes('<button type="button" data-layout="side-by-side" aria-pressed="false">Split</button>'), html)
  assert.ok(html.includes('</div><div class="diff-body"></div></div>'), html)
  assert.deepEqual(Array.from(app.run('pendingDiffs')), [diffText])
})

test('a tagged code fence shows its path and lines', () => {
  const app = markdownDashboard()
  app.context.text = '```rust path=a/b.rs lines=2-9\nfn main() {}\n```\n'
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="code"><div class="code-path">a/b.rs:2-9</div><pre><code class="hljs"><span class="hljs-keyword">fn</span>'), html)
})

test('a code fence in a known language shows highlight.js colours', () => {
  const app = markdownDashboard()
  app.context.text = "```javascript\nconst a = '<b>'\n```\n"
  const html = app.run("fileHtml('steps/01.md', text)")

  const code = '<span class="hljs-keyword">const</span> a = <span class="hljs-string">&#x27;&lt;b&gt;&#x27;</span>'
  assert.ok(html.includes(`<div class="code"><pre><code class="hljs">${code}</code></pre></div>`), html)
})

test('a code fence in an unknown language shows its text escaped', () => {
  const app = markdownDashboard()
  app.context.text = '```nope\n<b>\n```\n'
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="code"><pre><code class="hljs">&#60;b&#62;'), html)
})

const excerptSource = readFileSync(`${__dirname}/../../mcp/src/excerpt.rs`, 'utf8')

// Returns the language names that `language` in `app/mcp/src/excerpt.rs` returns.
function excerptLanguages() {
  const start = excerptSource.indexOf('pub fn language(')
  const end = excerptSource.indexOf('\n}\n', start)
  return [...excerptSource.slice(start, end).matchAll(/=> "([^"]+)"/g)].map((match) => match[1])
}

test('every language an excerpt names resolves in highlight.js', () => {
  const app = markdownDashboard()
  const languages = excerptLanguages()

  assert.ok(languages.includes('scala'), languages.join(', '))
  assert.deepEqual(languages.filter((language) => !app.context.hljs.getLanguage(language)), [])
})

const diff2htmlScript = readFileSync(`${__dirname}/diff2html-ui-base.min.js`, 'utf8')

// Returns the HTML that `drawDiff` draws for `text` through the vendored diff2html.
function diff2htmlHtml(app, text) {
  const body = { innerHTML: '', querySelector: () => null, querySelectorAll: () => [] }
  app.context.block = { diffText: text, querySelector: () => body }
  app.run("drawDiff(block, 'line-by-line')")
  return body.innerHTML
}

test('a diff fence without file headers shows as diff code', () => {
  const app = markdownDashboard()
  app.run(diff2htmlScript)
  const languages = []
  app.context.hljs.highlight = (text, { language }) => {
    languages.push(language)
    return { value: text }
  }
  const hunk = '@@ -1 +1 @@\n-old\n+new'
  assert.ok(!diff2htmlHtml(app, hunk).includes('d2h-file-wrapper'))

  app.context.text = `\`\`\`diff\n${hunk}\n\`\`\`\n`
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes(`<div class="code"><pre><code class="hljs">${hunk}`), html)
  assert.ok(!html.includes('class="diff"'), html)
  assert.deepEqual(Array.from(app.run('pendingDiffs')), [])
  assert.deepEqual(languages, ['diff'])
})

test('a diff fence with file headers draws a file through diff2html', () => {
  const app = markdownDashboard()
  app.run(diff2htmlScript)
  const unified = ['--- a/f.txt', '+++ b/f.txt', '@@ -1 +1 @@', '-old', '+new'].join('\n')

  for (const text of [diffText, unified]) {
    app.context.text = `\`\`\`diff\n${text}\n\`\`\`\n`
    const html = app.run("fileHtml('steps/01.md', text)")

    assert.ok(html.includes('<div class="diff" data-diff="0">'), html)
    assert.deepEqual(Array.from(app.run('pendingDiffs')), [text])
    assert.ok(diff2htmlHtml(app, text).includes('d2h-file-wrapper'), text)
  }
})

test('parses routes with the org', () => {
  const app = dashboard()
  assert.deepEqual(
    plain(app.run('parseRoute("#/org/a%20b%25c/fix/s1")')),
    { kind: 'session', org: 'org', repo: 'a b%c', branch: 'fix', id: 's1', path: null },
  )
  assert.deepEqual(
    plain(app.run('parseRoute("#/org/repo/main/s1/round-1/report.md")')),
    { kind: 'session', org: 'org', repo: 'repo', branch: 'main', id: 's1', path: 'round-1/report.md' },
  )

  app.context.session = { org: 'org', repo: 'a b%c', branch: 'fix', id: 's1' }
  assert.equal(app.run('sessionHref(session)'), '#/org/a%20b%25c/fix/s1')
  assert.equal(app.run('fileHref(session, "round-1/report.md")'), '#/org/a%20b%25c/fix/s1/round-1/report.md')
  assert.equal(app.run('sessionHref(parseRoute(sessionHref(session)))'), '#/org/a%20b%25c/fix/s1')
})

test('parses the new routes', () => {
  const app = dashboard()
  const route = (hash) => plain(app.run(`parseRoute(${JSON.stringify(hash)})`))

  assert.deepEqual(route(''), { kind: 'repos' })
  assert.deepEqual(route('#/'), { kind: 'repos' })
  assert.deepEqual(route('#/sessions'), { kind: 'sessions' })
  assert.deepEqual(route('#/worktree/%2Fa%2Fb%20c'), { kind: 'worktree', path: '/a/b c' })
  assert.deepEqual(route("#/prs/%2Fs%2Fo%2Fr%2Fwork%20tree's"), { kind: 'prs', path: "/s/o/r/work tree's" })
  assert.deepEqual(route('#/o/r/b/id/spec.md'), { kind: 'session', org: 'o', repo: 'r', branch: 'b', id: 'id', path: 'spec.md' })

  assert.equal(app.run(`prsHash(${JSON.stringify("/s/o/r/work tree's")})`), "#/prs/%2Fs%2Fo%2Fr%2Fwork%20tree's")
})

test('a worktree route keeps a path with a space and a quote', () => {
  const app = dashboard()
  app.context.path = "/s/o/r/work tree's"

  assert.equal(app.run('worktreeHash(path)'), "#/worktree/%2Fs%2Fo%2Fr%2Fwork%20tree's")
  assert.deepEqual(plain(app.run('parseRoute(worktreeHash(path))')), { kind: 'worktree', path: "/s/o/r/work tree's" })
})

// Returns `html` with its `&#N;` entities decoded.
const unescapeHtml = (html) => html.replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code))

// Returns the text of `html`, without its tags, with its entities decoded and its spaces collapsed.
const textOf = (html) => unescapeHtml(html.replace(/<[^>]*>/g, ' ')).replace(/\s+/g, ' ').trim()

const pageText = (app) => textOf(app.elements.get('page').innerHTML)

// Returns the entry of `session` in the `api/sessions` reply.
const sessionEntry = ({ org, repo, branch, id, dir, ...data }) => ({ org, repo, branch, session: id, category: 'session', data })

// Returns a dashboard on `hash` after one poll of an api that lists `sessions` and answers `api/repos` with `repos`.
async function reposApp(repos, sessions = [], hash = '#/') {
  const app = dashboard({
    fetcher: (url) => {
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
  app.context.location.hash = hash
  await app.run('poll()')
  return app
}

// Returns the main rows of the repos page, each with its class, its decoded link or null, and its text.
const mainRows = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<div class="(row main[^"]*)">\s*<(?:a class="body" href="([^"]*)"|div class="body")>([\s\S]*?)<\/(?:a|div)>/g)]
  .map((match) => ({ class: match[1], href: match[2] === undefined ? null : unescapeHtml(match[2]), text: textOf(match[3]) }))

// Returns the worktree rows of the repos page, each with its decoded link and its text.
const worktreeRows = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<div class="row worktree[^"]*">\s*<a class="body" href="([^"]*)">([\s\S]*?)<\/a>/g)]
  .map((match) => ({ href: unescapeHtml(match[1]), text: textOf(match[2]) }))

const listText = (app) => textOf(app.elements.get('repo-list').innerHTML)

// Types `text` into the filter field of the repos page.
function typeFilter(app, text) {
  const field = app.elements.get('repo-filter')
  field.value = text
  field.dispatch('input')
}

test('the repos page lists checkouts and worktree links', async () => {
  const linked = "/s/akka/sdk/work tree's"
  const repos = {
    error: null,
    repos: [
      {
        org: 'akka',
        repo: 'sdk',
        path: '/s/akka/sdk/main',
        branch: 'main',
        error: null,
        worktrees: [
          { path: '/s/akka/sdk/main', branch: 'main', head: null },
          { path: linked, branch: null, head: '1234567' },
        ],
      },
      {
        org: 'octonato',
        repo: 'jam-factory',
        path: '/s/octonato/jam-factory/main',
        branch: null,
        error: null,
        worktrees: [{ path: '/s/octonato/jam-factory/main', branch: 'fix/<b>', head: null }],
      },
    ],
  }
  const at = (id, worktree) => ({ ...session, id, worktree })
  const sessions = [at('s1', '/s/akka/sdk/main'), at('s2', '/s/akka/sdk/main/'), at('s3', linked), at('s4', '/s/akka/sdk')]
  const app = await reposApp(repos, sessions)
  const list = app.elements.get('repo-list').innerHTML

  // Returns the number of worktree rows in each nested block of the repos page.
  const nestedCounts = [...list.matchAll(/<div class="rows nested">([\s\S]*?)<\/section>/g)]
    .map((match) => [...match[1].matchAll(/<div class="row worktree/g)].length)

  assert.ok(app.calls.includes('api/repos'))
  assert.deepEqual(plain(app.run('reposState')), repos)
  assert.deepEqual(mainRows(app), [
    { class: 'row main', href: '#/prs/%2Fs%2Fakka%2Fsdk%2Fmain', text: 'akka/sdk main PRs ›' },
    { class: 'row main', href: '#/prs/%2Fs%2Foctonato%2Fjam-factory%2Fmain', text: 'octonato/jam-factory PRs ›' },
  ])
  assert.deepEqual(worktreeRows(app), [
    { href: '#/worktree/%2Fs%2Fakka%2Fsdk%2Fmain', text: 'main akka/sdk/main 2 sessions' },
    { href: "#/worktree/%2Fs%2Fakka%2Fsdk%2Fwork%20tree's", text: "1234567 akka/sdk/work tree's 1 session" },
    { href: '#/worktree/%2Fs%2Foctonato%2Fjam-factory%2Fmain', text: 'fix/<b> octonato/jam-factory/main 0 sessions' },
  ])
  assert.deepEqual(nestedCounts, [2, 1])
  assert.doesNotMatch(list, /<b>/)
})

test('the repos page explains an empty config', async () => {
  const app = await reposApp({ error: null, repos: [] })

  assert.ok(listText(app).includes('~/.jam-factory/config.json'), listText(app))
  assert.ok(listText(app).includes('"repos"'), listText(app))
  assert.deepEqual(worktreeRows(app), [])
})

test('a checkout with an error is greyed with the reason', async () => {
  const repos = { error: null, repos: [{ org: 'o', repo: 'r', path: '/s/o/r/main', branch: null, error: 'not a git repository', worktrees: [] }] }
  const app = await reposApp(repos)

  assert.deepEqual(mainRows(app), [{ class: 'row main error', href: null, text: 'o/r o/r/main not a git repository' }])
  assert.deepEqual(worktreeRows(app), [])
  assert.deepEqual(plain(app.run('shownRows()')), [])
  assert.match(html, /\.row\.main\.error \{[^}]*color: var\(--muted\);/)

  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), null)
})

test('a checkout without worktrees says so', async () => {
  const repos = { error: null, repos: [{ org: 'o', repo: 'r', path: '/s/o/r/main', branch: 'main', error: null, worktrees: [] }] }
  const app = await reposApp(repos)

  assert.deepEqual(mainRows(app), [{ class: 'row main', href: '#/prs/%2Fs%2Fo%2Fr%2Fmain', text: 'o/r main PRs ›' }])
  assert.equal(listText(app), 'o/r main PRs › No worktrees.')
})

const filterRepos = {
  error: null,
  repos: [
    {
      org: 'akka',
      repo: 'sdk',
      path: '/s/akka/sdk/main',
      branch: 'main',
      error: null,
      worktrees: [
        { path: '/s/akka/sdk/rate-limit', branch: 'rate-limit', head: null },
        { path: '/s/akka/sdk/wt', branch: 'feature', head: null },
      ],
    },
    { org: 'o', repo: 'r', path: '/s/o/r/main', branch: 'main', error: null, worktrees: [{ path: '/s/o/r/feature', branch: 'feature', head: null }] },
  ],
}

// Returns the texts of the worktree rows of the repos page.
const rowTexts = (app) => worktreeRows(app).map((row) => row.text)

test('the filter shows the worktrees whose short path or branch holds every word', async () => {
  const app = await reposApp(filterRepos)

  typeFilter(app, 'FEAT')
  assert.deepEqual(rowTexts(app), ['feature akka/sdk/wt 0 sessions', 'feature o/r/feature 0 sessions'])
  assert.deepEqual(mainRows(app).map((row) => row.text), ['akka/sdk main PRs ›', 'o/r main PRs ›'])

  typeFilter(app, 'sdk feat')
  assert.deepEqual(rowTexts(app), ['feature akka/sdk/wt 0 sessions'])
  assert.deepEqual(mainRows(app).map((row) => row.text), ['akka/sdk main PRs ›'])

  typeFilter(app, '  ')
  assert.deepEqual(rowTexts(app), [
    'rate-limit akka/sdk/rate-limit 0 sessions',
    'feature akka/sdk/wt 0 sessions',
    'feature o/r/feature 0 sessions',
  ])
})

test('a checkout whose name matches shows without a matching worktree', async () => {
  const docs = { org: 'akka', repo: 'docs', path: '/s/akka/docs/main', branch: 'main', error: null, worktrees: [] }
  const repos = { error: null, repos: [...filterRepos.repos, docs] }
  const app = await reposApp(repos)

  typeFilter(app, 'docs')
  assert.deepEqual(mainRows(app), [{ class: 'row main', href: '#/prs/%2Fs%2Fakka%2Fdocs%2Fmain', text: 'akka/docs main PRs ›' }])
  assert.equal(listText(app), 'akka/docs main PRs › No worktrees.')
})

test('a filter that hides everything says so', async () => {
  const app = await reposApp(filterRepos)

  typeFilter(app, 'zzz')
  assert.equal(listText(app), 'No worktree matches.')
})

test('a poll keeps the filter field', async () => {
  const app = await reposApp(filterRepos)
  const field = app.elements.get('repo-filter')

  typeFilter(app, 'sdk')
  await app.run('poll()')
  assert.equal(app.elements.get('repo-filter'), field)
  assert.equal(field.value, 'sdk')
  assert.deepEqual(rowTexts(app), ['rate-limit akka/sdk/rate-limit 0 sessions', 'feature akka/sdk/wt 0 sessions'])
})

test('the filter survives leaving and returning', async () => {
  const app = await reposApp(filterRepos)
  typeFilter(app, 'sdk')

  navigate(app, worktreeHashOf('/s/akka/sdk/wt'))
  assert.match(app.elements.get('page').innerHTML, /id="worktree-card"/)

  navigate(app, '#/')
  assert.equal(app.elements.get('repo-filter').value, 'sdk')
  assert.deepEqual(rowTexts(app), ['rate-limit akka/sdk/rate-limit 0 sessions', 'feature akka/sdk/wt 0 sessions'])
})

test('a config error is shown', async () => {
  const app = await reposApp({ error: 'bad', repos: [] })

  assert.match(listText(app), /\bbad\b/)
  assert.doesNotMatch(listText(app), /config\.json/)
})

test('a failed repos fetch keeps the sessions and shows the failure', async () => {
  const app = dashboard({
    fetcher: (url) => (url === 'api/sessions' ? sessionsReply([sessionEntry(session)]) : response('', false, { status: 500 })),
  })
  await app.run('poll()')

  assert.equal(app.elements.get('status').textContent, '1 sessions')
  assert.match(listText(app), /500 api\/repos/)
})

test('sessions match a worktree with or without a trailing slash', () => {
  const app = dashboard()
  const at = (id, worktree) => ({ ...session, id, dir: `api/file/org/repo/main/${id}/`, worktree })
  app.context.sessions = [at('a', '/w/main'), at('b', '/w/main/'), at('c', '/w/main2'), at('d', undefined), at('e', '/w')]
  app.run('state.sessions = sessions')
  const ids = (path) => Array.from(app.run(`sessionsOf(${JSON.stringify(path)})`), (s) => s.id)

  assert.deepEqual(ids('/w/main'), ['a', 'b'])
  assert.deepEqual(ids('/w/main/'), ['a', 'b'])
  assert.deepEqual(ids('/w'), ['e'])
  assert.equal(app.run('sameWorktree("/a//", "/a")'), false)
})

test('the sessions tab lists sessions', async () => {
  const app = await reposApp({ error: null, repos: [] }, [session], '#/sessions')
  const page = app.elements.get('page').innerHTML

  assert.deepEqual(cardRows(page).map((row) => row.dataset.dir), [session.dir])
  assert.match(page, /<h2 class="org">org<\/h2>/)
})

test('the tab strip marks the shown page', async () => {
  // Returns the tabs as their label and link, with `*` after the current one.
  const tabs = (app) => [...app.elements.get('tabs').innerHTML.matchAll(/<a class="tab" href="([^"]*)"( aria-current="page")?>([^<]*)<\/a>/g)]
    .map((match) => `${match[3]} ${match[1]}${match[2] ? '*' : ''}`)

  const app = await reposApp({ error: null, repos: [] }, [session])
  assert.deepEqual(tabs(app), ['Repos #/*', 'Sessions #/sessions', 'Terminals #/terminals'])

  const shown = [
    ['#/sessions', ['Repos #/', 'Sessions #/sessions*', 'Terminals #/terminals']],
    ['#/terminals', ['Repos #/', 'Sessions #/sessions', 'Terminals #/terminals*']],
    [app.run('sessionHref(state.sessions[0])'), ['Repos #/', 'Sessions #/sessions', 'Terminals #/terminals']],
    ['#/worktree/%2Fa', ['Repos #/', 'Sessions #/sessions', 'Terminals #/terminals']],
    ['', ['Repos #/*', 'Sessions #/sessions', 'Terminals #/terminals']],
  ]
  for (const [hash, expected] of shown) {
    app.context.location.hash = hash
    app.window.dispatch('hashchange')
    assert.deepEqual(tabs(app), expected, hash)
  }
})

test('groups sessions by org and filters them', () => {
  const at = (org, id, updated) => ({ ...session, org, id, dir: `api/file/${org}/repo/main/${id}/`, updated })
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
  assert.deepEqual(posts, ['api/archive/my%20org/a%20b%25c/main/s1', 'api/open/zed/my%20org/a%20b%25c/main/s1'])
  assert.match(app.context.messages[0], /^Archive my org\/a b%c\/main\/s1\?/)
  assert.equal(app.context.location.hash, '#/sessions')

  const failing = dashboard({ fetcher: () => response('zed is missing', false) })
  failing.context.messages = []
  failing.context.session = app.context.session
  failing.run('tell = async (message) => { messages.push(message) }')
  await failing.run('openTool(session, "zed")')
  assert.deepEqual(failing.context.messages, ['Could not open my org/a b%c/main/s1 in Zed: zed is missing'])
})

test('the archive action draws an archive box', () => {
  const app = dashboard()
  const icon = app.run("CARD_ACTIONS.find((entry) => entry.action === 'archive').icon")
  assert.equal(icon, '<rect x="2" y="2.5" width="12" height="3" rx=".5"/><path d="M3 5.5v8h10v-8M6.5 8h3"/>')
})

// Dispatches a keydown for `key` with `modifiers` on `target` and returns true when the page took it.
function press(app, key, modifiers = {}, target = { closest: () => null }) {
  let prevented = false
  app.context.document.dispatch('keydown', {
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...modifiers,
    target,
    preventDefault: () => { prevented = true },
  })
  return prevented
}

test('l, Escape and Cmd+[ return from a worktree page to the repos', () => {
  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const app = dashboard()
    app.context.location.hash = '#/worktree/%2Fa'

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, '#/')
  }
})

test('other modified keys, an open dialog, the list and the repos page ignore the back keys', () => {
  const app = dashboard()
  app.context.session = session
  app.run('state.sessions = [session]')
  const href = app.run('sessionHref(session)')
  app.context.location.hash = href

  for (const modifiers of [{ ctrlKey: true }, { metaKey: true, altKey: true }, { metaKey: true, shiftKey: true }]) {
    assert.equal(press(app, '[', modifiers), false)
  }
  assert.equal(press(app, 'l', { metaKey: true }), false)
  assert.equal(app.context.location.hash, href)

  app.elements.get('modal').open = true
  assert.equal(press(app, 'Escape'), false)
  assert.equal(app.context.location.hash, href)

  app.elements.get('modal').open = false
  app.context.location.hash = '#/sessions'
  assert.equal(press(app, 'Escape'), false)
  assert.equal(press(app, '[', { metaKey: true }), false)

  app.context.location.hash = '#/'
  assert.equal(press(app, 'Escape'), false)
  assert.equal(press(app, 'l'), false)
})

const shortcutsDialog = (app) => app.elements.get('shortcuts')
const shortcutsText = (app) => textOf(app.elements.get('shortcuts-list').innerHTML)

test('? opens the shortcuts dialog on every page, and ? closes it', () => {
  for (const hash of ['#/', '#/sessions', '#/terminals', '#/worktree/%2Fa', prsHashOf(sdkMain)]) {
    const app = dashboard()
    app.context.location.hash = hash

    assert.ok(press(app, '?', { shiftKey: true }), `? is taken on ${hash}`)
    assert.equal(shortcutsDialog(app).open, true)
    assert.match(shortcutsText(app), /^Everywhere \? Show or hide the shortcuts/)
    assert.match(shortcutsText(app), /PRs page l Esc ⌘\[ Go back to the repos/)

    assert.ok(press(app, '?', { shiftKey: true }))
    assert.equal(shortcutsDialog(app).open, false)
    assert.equal(app.context.location.hash, hash)
  }
})

test('the open shortcuts dialog keeps the page keys from the page', () => {
  const app = dashboard()
  app.context.location.hash = '#/worktree/%2Fa'
  press(app, '?', { shiftKey: true })

  assert.equal(press(app, 'l'), false)
  assert.equal(press(app, '[', { metaKey: true }), false)
  assert.equal(app.context.location.hash, '#/worktree/%2Fa')
  assert.equal(shortcutsDialog(app).open, true)
})

test('? in a text field, a terminal or an open dialog opens no shortcuts', () => {
  const app = dashboard()
  app.context.location.hash = '#/'
  const field = { closest: (selector) => (selector.includes('input') ? {} : null) }
  const terminal = { closest: (selector) => (selector === '#term-view' ? {} : null) }

  assert.equal(press(app, '?', { shiftKey: true }, field), false)
  assert.equal(press(app, '?', { shiftKey: true }, terminal), false)

  app.elements.get('modal').open = true
  assert.equal(press(app, '?', { shiftKey: true }), false)
  assert.notEqual(shortcutsDialog(app).open, true)
})

test('the shortcuts dialog lists every key of the key tables', () => {
  const app = dashboard()
  const sections = plain(app.run('SHORTCUTS'))
  const keysOf = (title) => sections.find((section) => section.title === title).rows.flatMap((row) => row.keys)
  const tables = plain(app.run('({ ACTION_KEYS, WORKTREE_KEYS, PR_KEYS, keys: [NEW_WORKTREE_KEY, REMOVE_WORKTREE_KEY] })'))

  const expected = [
    ['Sessions', Object.keys(tables.ACTION_KEYS)],
    ['Repos', [...Object.keys(tables.WORKTREE_KEYS), ...tables.keys]],
    ['Worktree page', [...Object.keys(tables.WORKTREE_KEYS), tables.keys[1]]],
    ['Session item', [...Object.keys(tables.WORKTREE_KEYS), tables.keys[1]]],
    ['PRs page', [...Object.keys(tables.PR_KEYS), tables.keys[0]]],
  ]
  for (const [title, keys] of expected) {
    for (const key of keys) assert.ok(keysOf(title).includes(key), `${title} lists ${key}`)
  }
})

const [pathA, pathB, pathC] = ['/s/akka/sdk/alpha', '/s/akka/sdk/beta', '/s/o/r/gamma']
const [sdkMain, orMain] = ['/s/akka/sdk/main', '/s/o/r/main']
const selectRepos = {
  error: null,
  repos: [
    {
      org: 'akka',
      repo: 'sdk',
      path: sdkMain,
      branch: 'main',
      error: null,
      worktrees: [
        { path: pathA, branch: 'alpha', head: null },
        { path: pathB, branch: 'beta', head: null },
      ],
    },
    { org: 'o', repo: 'r', path: orMain, branch: 'main', error: null, worktrees: [{ path: pathC, branch: 'gamma', head: null }] },
  ],
}

// Returns the hash of the PRs page of `path`.
const prsHashOf = (path) => `#/prs/${encodeURIComponent(path)}`

// Returns the decoded links of the outlined main and worktree rows of the repos page.
const selectedRows = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<div class="row (?:main|worktree) selected">\s*<a class="body" href="([^"]*)">/g)]
  .map((match) => unescapeHtml(match[1]))

test('arrow keys move the selection across main rows and worktree rows', async () => {
  const app = await reposApp(selectRepos)
  const steps = [
    [sdkMain, prsHashOf(sdkMain)],
    [pathA, worktreeHashOf(pathA)],
    [pathB, worktreeHashOf(pathB)],
    [orMain, prsHashOf(orMain)],
    [pathC, worktreeHashOf(pathC)],
    [pathC, worktreeHashOf(pathC)],
  ]

  for (const [path, hash] of steps) {
    assert.ok(press(app, 'ArrowDown'))
    assert.equal(app.run('selectedWorktree'), path)
    assert.deepEqual(selectedRows(app), [hash])
  }

  assert.ok(press(app, 'ArrowUp'))
  assert.equal(app.run('selectedWorktree'), orMain)
  assert.match(app.elements.get('repo-list').innerHTML, /<div class="row main selected">/)

  const fresh = await reposApp(selectRepos)
  assert.ok(press(fresh, 'ArrowUp'))
  assert.equal(fresh.run('selectedWorktree'), pathC)
  assert.deepEqual(selectedRows(fresh), [worktreeHashOf(pathC)])
})

test('Enter opens the selected worktree', async () => {
  const app = await reposApp(selectRepos)

  assert.equal(press(app, 'Enter'), false)
  assert.equal(app.context.location.hash, '#/')

  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
})

// Gives `app` a fake app API and returns the arguments of each `open_window` call.
function appOpener(app) {
  const opened = []
  app.window.__TAURI__ = {
    core: {
      invoke: async (command, args) => {
        assert.equal(command, 'open_window')
        opened.push(args.hash)
      },
    },
  }
  return opened
}

// Clicks the link `href` of a `kind` row in the element `container` with `modifiers`, and returns true when the page prevented the default.
function clickRow(app, kind, href, modifiers = {}, container = 'repo-list') {
  let prevented = false
  const link = { getAttribute: (name) => (name === 'href' ? href : null) }
  const target = { closest: (selector) => (selector.includes(`.row.${kind} > a.body`) ? link : null) }

  app.elements.get(container).dispatch('click', { target, ...modifiers, preventDefault: () => { prevented = true } })
  return prevented
}

test('Cmd+click on a main or worktree row opens its page in a new app window', async () => {
  const app = await reposApp(selectRepos)
  const opened = appOpener(app)

  assert.ok(clickRow(app, 'main', prsHashOf(sdkMain), { metaKey: true }))
  assert.ok(clickRow(app, 'worktree', worktreeHashOf(pathA), { metaKey: true }))
  await settle()

  assert.deepEqual(opened, [prsHashOf(sdkMain), worktreeHashOf(pathA)])
  assert.equal(app.context.location.hash, '#/')
})

test('a plain click on a row follows its link in the same window', async () => {
  const app = await reposApp(selectRepos)
  const opened = appOpener(app)

  assert.equal(clickRow(app, 'worktree', worktreeHashOf(pathA)), false)
  assert.deepEqual(opened, [])
})

test('Cmd+click on a row in a browser keeps the browser behaviour', async () => {
  const app = await reposApp(selectRepos)

  assert.equal(clickRow(app, 'worktree', worktreeHashOf(pathA), { metaKey: true }), false)
})

test('Enter on a main row opens its PRs page', async () => {
  const app = await reposApp(selectRepos)

  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), sdkMain)
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, '#/prs/%2Fs%2Fakka%2Fsdk%2Fmain')
})

test('the worktree keys do nothing on a main row', async () => {
  const { app, posts, sockets } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(selectRepos)))
  navigate(app, '#/')

  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), sdkMain)
  for (const key of ['o', 'i', 'e', 'g', 't']) assert.equal(press(app, key), false, `${key} does nothing`)
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(sockets.length, 0)
  assert.equal(app.context.location.hash, '#/')
})

test('a filtered-out or removed worktree loses the selection', async () => {
  const repos = plain(selectRepos)
  const app = await reposApp(repos)

  for (let i = 0; i < 3; i += 1) press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathB)
  typeFilter(app, 'alpha')
  assert.equal(app.run('selectedWorktree'), null)
  assert.deepEqual(selectedRows(app), [])

  typeFilter(app, '')
  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathA)
  repos.repos[0].worktrees.shift()
  await app.run('poll()')
  assert.equal(app.run('selectedWorktree'), null)
  assert.deepEqual(selectedRows(app), [])
})

test('a poll keeps the selection', async () => {
  const app = await reposApp(selectRepos)
  for (let i = 0; i < 3; i += 1) press(app, 'ArrowDown')

  await app.run('poll()')
  assert.equal(app.run('selectedWorktree'), pathB)
  assert.deepEqual(selectedRows(app), [worktreeHashOf(pathB)])
})

test('slash focuses the filter', async () => {
  const app = await reposApp(selectRepos)

  assert.ok(press(app, '/'))
  assert.equal(app.elements.get('repo-filter').focused, true)
})

test('keys in the filter field', async () => {
  const app = await reposApp(selectRepos)
  const field = app.elements.get('repo-filter')

  field.focus()
  assert.ok(press(app, 'ArrowDown', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.run('selectedWorktree'), sdkMain)

  field.focus()
  for (const key of ['t', 'l']) assert.equal(press(app, key, {}, field), false, `${key} is text`)
  assert.equal(field.focused, true)
  assert.equal(app.run('selectedWorktree'), sdkMain)
  assert.equal(app.context.location.hash, '#/')

  assert.ok(press(app, 'Escape', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.context.location.hash, '#/')

  press(app, 'ArrowDown')
  field.focus()
  assert.ok(press(app, 'Enter', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))

  const fresh = await reposApp(selectRepos)
  const freshField = fresh.elements.get('repo-filter')
  freshField.focus()
  assert.ok(press(fresh, 'Enter', {}, freshField))
  assert.equal(freshField.focused, false)
  assert.equal(fresh.run('selectedWorktree'), pathA)
  assert.equal(fresh.context.location.hash, worktreeHashOf(pathA))
})

test('Enter in the filter opens the first matching worktree', async () => {
  const app = await reposApp(selectRepos)
  const field = app.elements.get('repo-filter')

  typeFilter(app, 'beta')
  field.focus()
  assert.ok(press(app, 'Enter', {}, field))
  assert.equal(app.run('selectedWorktree'), pathB)
  assert.equal(app.context.location.hash, worktreeHashOf(pathB))
})

test('Enter in a filter that matches only a checkout name opens its PRs page', async () => {
  const docsMain = '/s/akka/docs/main'
  const docs = { org: 'akka', repo: 'docs', path: docsMain, branch: 'main', error: null, worktrees: [] }
  const app = await reposApp({ error: null, repos: [...selectRepos.repos, docs] })
  const field = app.elements.get('repo-filter')

  typeFilter(app, 'docs')
  field.focus()
  assert.ok(press(app, 'Enter', {}, field))
  assert.equal(app.run('selectedWorktree'), docsMain)
  assert.equal(app.context.location.hash, prsHashOf(docsMain))
})

test('a back key on a worktree page selects it on the repos page', async () => {
  const app = await reposApp(selectRepos, [], worktreeHashOf(pathB))

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), pathB)

  app.window.dispatch('hashchange')
  assert.deepEqual(selectedRows(app), [worktreeHashOf(pathB)])
})

const editSession = { ...session, org: 'o', repo: 'r', id: 's', dir: 'api/file/o/r/main/s/' }

// Waits until the pending fetches and renders are done.
const settle = () => new Promise(setImmediate)

// Returns a fake board API over the files of `editSession`. GET answers a file with its version as `ETag`. PUT checks `If-Match` and records the call.
function fileApi(files) {
  const versions = new Map(Object.keys(files).map((path) => [path, 1]))
  const puts = []
  const etag = (path) => `"v${versions.get(path)}"`

  // Changes `path` on disk to `text`.
  const touch = (path, text) => {
    files[path] = text
    versions.set(path, versions.get(path) + 1)
  }

  const fetcher = (url, options = {}) => {
    if (url === 'api/files/o/r/main/s') return filesReply(Object.keys(files))

    const path = url.slice(editSession.dir.length)
    if (!Object.hasOwn(files, path)) return response('', false, { status: 404 })
    if (options.method !== 'PUT') return response(files[path], true, { headers: { ETag: etag(path) } })

    const ifMatch = options.headers['If-Match']
    puts.push({ url, ifMatch, body: options.body })
    if (ifMatch !== etag(path)) return response('', false, { status: 409, headers: { ETag: etag(path) } })

    touch(path, options.body)
    return response('', true, { headers: { ETag: etag(path) } })
  }
  return { fetcher, files, puts, touch }
}

// Returns a dashboard that shows `path` of `editSession` from `api`.
async function openFile(api, path) {
  const app = dashboard({ fetcher: api.fetcher })
  app.context.session = editSession
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run(`fileHref(session, ${JSON.stringify(path)})`)
  app.run('render()')
  await settle()
  return app
}

// Returns the editor field in the pane, or undefined when the pane holds none.
const editor = (app) => app.elements.get('view').children[0]

// Closes the open modal with the button of `value`.
function closeModal(app, value) {
  const modal = app.elements.get('modal')
  modal.returnValue = value
  modal.open = false
  modal.dispatch('close')
}

test('w opens the editor on markdown', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')

  assert.ok(press(app, 'w'))
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(app.run('editing.editor.value()'), '# Spec\n')
  assert.equal(editor(app).tagName, 'TEXTAREA')
  assert.equal(editor(app).focused, true)
})

test('w ignores files that are not markdown', async () => {
  for (const path of ['runs.jsonl', 'session.json']) {
    const app = await openFile(fileApi({ [path]: '{}\n' }), path)
    assert.match(app.elements.get('view').innerHTML, /\{\}/)

    press(app, 'w')
    assert.equal(app.run('editing'), null)
    assert.equal(editor(app), undefined)
  }
})

test('w while a new file loads leaves the editor closed', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n', 'plan.md': '# Plan\n' })
  let release = null
  const held = new Promise((resolve) => { release = resolve })
  const fetcher = (url, options) => (url.endsWith('/plan.md') ? held.then(() => api.fetcher(url, options)) : api.fetcher(url, options))
  const app = await openFile({ fetcher }, 'spec.md')

  app.context.location.hash = app.run('fileHref(session, "plan.md")')
  app.window.dispatch('hashchange')
  await settle()
  assert.match(app.elements.get('viewer-head').innerHTML, /plan\.md/)

  press(app, 'w')
  assert.equal(app.run('editing'), null)
  assert.equal(editor(app), undefined)

  release()
  await settle()
  assert.match(app.elements.get('view').innerHTML, /# Plan/)

  press(app, 'w')
  assert.equal(app.run('editing.path'), 'plan.md')
  assert.equal(app.run('editing.editor.value()'), '# Plan\n')
})

test('Escape in the editor without changes returns to the view without a PUT', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  assert.ok(press(app, 'Escape', {}, editor(app)))
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing'), null)
  assert.equal(editor(app), undefined)
  assert.match(app.elements.get('view').innerHTML, /# Spec/)
  assert.equal(app.context.location.hash, href)
})

test('a CRLF file without typing leaves the editor without a PUT or a discard prompt', async () => {
  const api = fileApi({ 'spec.md': '# Spec\r\nBody\r\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  assert.equal(app.run('hasUnsavedChanges()'), false)
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing'), null)

  press(app, 'w')
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.ok(press(app, 'l'))
  await settle()

  assert.equal(app.elements.get('modal').open, undefined)
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/')
  assert.deepEqual(api.puts, [])
})

test('Escape in the editor with changes puts them with If-Match, then shows the new text', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.deepEqual(api.puts, [{ url: 'api/file/o/r/main/s/spec.md', ifMatch: '"v1"', body: '# Edited\n' }])
  assert.equal(api.files['spec.md'], '# Edited\n')
  assert.equal(app.run('editing'), null)
  assert.match(app.elements.get('view').innerHTML, /# Edited/)
})

test('Cmd+S in the editor saves and stays in edit mode', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  assert.ok(press(app, 's', { metaKey: true }, editor(app)))
  await settle()

  assert.equal(api.puts.length, 1)
  assert.equal(api.files['spec.md'], '# Edited\n')
  assert.equal(app.run('editing.version'), 'v2')
  assert.equal(app.run('editing.original'), '# Edited\n')
  assert.equal(editor(app).value, '# Edited\n')
})

test('v outside the editor does not leave edit mode', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  const field = editor(app)
  field.value = '# Edited\n'
  assert.equal(press(app, 'v'), false)
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(editor(app), field)
  assert.equal(app.run('editing.editor.value()'), '# Edited\n')
})

test('Escape outside the editor leaves the page', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')

  press(app, 'w')
  assert.ok(press(app, 'Escape'))

  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/')
})

test('a conflict keeps edit mode and the text, and shows a message in the viewer head', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  api.touch('spec.md', '# Changed on disk\n')
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.equal(api.puts.length, 1)
  assert.equal(api.files['spec.md'], '# Changed on disk\n')
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(app.run('editing.editor.value()'), '# Edited\n')
  assert.equal(editor(app).value, '# Edited\n')
  assert.match(
    app.elements.get('viewer-head').innerHTML,
    /File changed on disk\. Copy your text, click outside the editor, leave the file and discard, then reopen it and press w\./,
  )
})

test('a poll while editing does not replace the editor', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  const field = editor(app)
  field.value = '# Typed\n'
  api.touch('spec.md', '# Changed on disk\n')
  app.run('render()')
  await settle()

  assert.equal(editor(app), field)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.doesNotMatch(app.elements.get('view').innerHTML, /Changed on disk/)
})

test('leaving with changes asks to discard them', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.ok(press(app, 'l'))
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.equal(app.context.location.hash, href)

  press(app, 'l')
  closeModal(app, 'ok')
  await settle()
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/')
  assert.deepEqual(api.puts, [])
})

test('a route change with changes asks to discard them and restores the route on cancel', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  const field = editor(app)
  field.value = '# Typed\n'
  app.context.location.hash = '#/'
  app.window.dispatch('hashchange')
  assert.equal(app.elements.get('modal').open, true)

  app.run('render()')
  await settle()
  assert.match(app.elements.get('page').innerHTML, /class="session-page"/, 'a poll keeps the session page while the modal is open')
  assert.equal(editor(app), field)

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, href)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
})

test('a stage change while editing the default file keeps the edited file in the pane', async () => {
  const app = dashboard({ fetcher: fileApi({ 'spec.md': '# Spec\n', 'plan.md': '# Plan\n' }).fetcher })
  app.context.session = { ...editSession, stage: 'spec' }
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('sessionHref(session)')
  app.run('render()')
  await settle()

  press(app, 'w')
  const field = editor(app)
  app.run('state.sessions = [{ ...session, stage: "plan" }]')
  app.run('render()')
  await settle()

  assert.equal(editor(app), field)
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.match(app.elements.get('viewer-head').innerHTML, /spec\.md/)
  assert.doesNotMatch(app.elements.get('viewer-head').innerHTML, /plan\.md/)
})

// Gives `app` a fake app window API and returns the count of its close calls.
function appWindow(app) {
  const closes = { count: 0 }
  app.window.__TAURI__ = { window: { getCurrentWindow: () => ({ close: async () => { closes.count += 1 } }) } }
  return closes
}

test('closing the app window without changes closes it at once', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const closes = appWindow(app)

  assert.equal(app.run('onCloseRequested()'), true)

  press(app, 'w')
  assert.equal(app.run('onCloseRequested()'), true)
  assert.equal(app.elements.get('modal').open, undefined)
  assert.equal(closes.count, 0)
})

test('closing the app window with changes asks to discard them, and closes it on discard', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const closes = appWindow(app)

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.equal(app.run('onCloseRequested()'), false)
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')
  assert.equal(app.run('onCloseRequested()'), false, 'a second request while the modal is open')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(closes.count, 0)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')

  assert.equal(app.run('onCloseRequested()'), false)
  closeModal(app, 'ok')
  await settle()
  assert.equal(closes.count, 1)
  assert.equal(app.run('editing'), null)
  assert.equal(app.run('onCloseRequested()'), true)
  assert.deepEqual(api.puts, [])
})

// Dispatches `beforeunload` to `app` and returns true when the page asks to stay.
function unload(app) {
  let prevented = false
  app.window.dispatch('beforeunload', { preventDefault: () => { prevented = true } })
  return prevented
}

test('a reload or page close asks to stay only while the editor holds unsaved changes', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  assert.equal(unload(app), false)

  press(app, 'w')
  assert.equal(unload(app), false)

  editor(app).value = '# Typed\n'
  assert.equal(unload(app), true)

  press(app, 's', { metaKey: true }, editor(app))
  await settle()
  assert.equal(unload(app), false)
})

// Returns a fake diff block of index 0, its body, and its Unified and Split buttons.
function diffBlock() {
  const body = {}
  const block = {
    dataset: { diff: '0' },
    querySelector: (selector) => (selector === '.diff-body' ? body : null),
    querySelectorAll: () => buttons,
  }
  const button = (layout, pressed) => {
    const el = {
      dataset: { layout },
      attributes: { 'aria-pressed': pressed },
      setAttribute: (name, value) => { el.attributes[name] = value },
      closest: (selector) => (selector.includes('button') ? el : selector.startsWith('.diff[') ? block : null),
    }
    return el
  }
  const buttons = [button('line-by-line', 'true'), button('side-by-side', 'false')]
  return { block, body, buttons }
}

test('a shown diff draws through diff2html and redraws in the clicked layout', async () => {
  const app = markdownDashboard({ fetcher: fileApi({ 'steps/01.md': diffFence }).fetcher })
  const draws = []
  app.context.Diff2HtmlUI = class {
    constructor(body, text, config, hljs) { this.call = { body, text, config, hljs } }
    draw() { draws.push(this.call) }
  }

  app.context.session = editSession
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('fileHref(session, "steps/01.md")')
  app.run('render()')

  const view = app.elements.get('view')
  const { block, body, buttons } = diffBlock()
  view.querySelectorAll = (selector) => (selector === '.diff[data-diff]' ? [block] : [])
  await settle()

  assert.ok(view.innerHTML.includes('<div class="diff" data-diff="0">'), view.innerHTML)
  assert.equal(draws.length, 1)
  assert.equal(draws[0].body, body)
  assert.equal(draws[0].text, diffText)
  assert.deepEqual(plain(draws[0].config), { outputFormat: 'line-by-line', drawFileList: false, matching: 'lines', highlight: true })
  assert.equal(draws[0].hljs, app.context.hljs)

  app.run('pendingDiffs = []')
  view.dispatch('click', { target: buttons[1] })
  assert.equal(draws.length, 2)
  assert.equal(draws[1].text, diffText)
  assert.equal(draws[1].config.outputFormat, 'side-by-side')
  assert.deepEqual(buttons.map((button) => button.attributes['aria-pressed']), ['false', 'true'])
})

// Returns a walk session of `editSession` at `stage`, with `done` of `planned` steps written.
const walkSession = (stage, done = 1, planned = 2, fields = {}) => ({
  ...editSession,
  flow: 'walk',
  stage,
  counts: { stepsPlanned: planned, stepsDone: done },
  blocked: null,
  ...fields,
})

const walkJson = JSON.stringify({
  mode: 'changes',
  summary: 'Adds rate limiting.',
  steps: [
    { n: 1, title: 'A', summary: 'The limiter.' },
    { n: 2, title: 'B', summary: 'The wiring.' },
  ],
})

// Returns the stops of `session` over `files` and `walkJson`.
function walkStops(app, session, files) {
  app.context.session = session
  app.context.files = files
  app.context.walkText = walkJson
  return plain(app.run('buildStops(session, files, [], {}, parseWalk(walkText))'))
}

// Returns a dashboard that shows the session page of `session` from `api`.
async function openSession(api, session) {
  const app = dashboard({ fetcher: api.fetcher })
  app.context.session = session
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('sessionHref(session)')
  app.run('render()')
  await settle()
  return app
}

test('classes sessions by flow', () => {
  const app = dashboard()
  const classOf = (flow) => {
    app.context.session = { ...session, flow }
    return app.run('sessionClass(session)')
  }

  assert.equal(classOf('build'), 'implementation')
  assert.equal(classOf('vibe'), 'implementation')
  assert.equal(classOf('review'), 'review')
  assert.equal(classOf('walk'), 'walk-through')
  assert.equal(classOf(undefined), 'implementation')
  assert.equal(classOf('other'), 'other')

  app.context.session = session
  assert.match(app.run('rowContent(session)'), /<span class="label"><span class="pill">implementation<\/span> /)
})

test('a row names its branch and run and shows the folder category', () => {
  const app = dashboard()
  app.context.session = { ...session, flow: 'walk', category: 'walk', id: 'walk-20261104-0912-3f00486', worktree: '/a/b/c/d' }
  const row = app.run('rowContent(session)')

  assert.match(row, /<span class="id">org\/repo\/main<\/span>/)
  assert.match(row, /<span class="path">walk-20261104-0912-3f00486 · b\/c\/d<\/span>/)
  assert.match(row, /<span class="pill">walk<\/span>/)

  app.context.session = { ...session, category: 'review' }
  assert.equal(app.run('sessionClass(session)'), 'review')
})

test('walk stops follow walk.json and the step files', () => {
  const app = dashboard()
  const stops = walkStops(app, walkSession('walk'), ['overview.md', 'walk.json', 'steps/01.md'])

  assert.deepEqual(stops.map((stop) => stop.label), ['Overview', 'Step 1: A', 'Step 2: B'])
  assert.deepEqual(stops.map((stop) => stop.file), ['overview.md', 'steps/01.md', null])
  assert.deepEqual(stops.map((stop) => stop.state), ['done', 'done', 'writing'])
  assert.deepEqual(stops.map((stop) => stop.summary), ['Adds rate limiting.', 'The limiter.', 'The wiring.'])

  app.context.stop = stops[2]
  const card = app.run('stopCardHtml(stop, null, false)')
  assert.match(card, /<span class="title">Step 2: B<\/span>/)
  assert.match(card, /<div class="card-foot"><span class="badge">writing<\/span><\/div>\s*<\/div>$/)
  assert.match(card, /<div class="note">The wiring\.<\/div>/)
  assert.doesNotMatch(card, /data-path=/)
  assert.match(html, /\.stop\.writing \{ --tone: var\(--accent\); \}/)
})

test('a done walk marks every written step done', () => {
  const app = dashboard()
  const done = walkSession('done', 2, 2)

  assert.deepEqual(walkStops(app, done, ['overview.md', 'steps/01.md', 'steps/02.md']).map((stop) => stop.state), ['done', 'done', 'done'])
  assert.deepEqual(walkStops(app, done, ['overview.md', 'steps/01.md']).map((stop) => stop.state), ['done', 'done', 'pending'])
  assert.equal(app.run('stepLabel(session)'), 'walk-through · done')
})

test('a blocked walk shows its reason on the overview', () => {
  const app = dashboard()
  const blocked = walkSession('blocked', 1, 2, { blocked: { stage: 'walk', reason: 'git failed' } })
  const stops = walkStops(app, blocked, ['overview.md', 'steps/01.md'])

  assert.deepEqual(stops.map((stop) => stop.state), ['blocked', 'done', 'pending'])
  assert.equal(stops[0].file, 'overview.md')
  assert.equal(stops[0].note, 'git failed')
  assert.deepEqual(stops[0].links, [{ label: 'session.json', path: 'session.json' }])
  assert.equal(app.run('stepLabel(session)'), 'walk-through · blocked')
  assert.equal(app.run('defaultFile(session, [], 0)'), 'session.json')
  assert.equal(app.run('stopIndexForFile(buildStops(session, files, [], {}, parseWalk(walkText)), "session.json")'), 0)
})

test('a walk without walk.json shows only the overview', async () => {
  const app = dashboard({ fetcher: () => response('', false) })
  for (const text of ['{', '{}', 'null', '7', '{"steps":{}}']) {
    assert.equal(app.run(`parseWalk(${JSON.stringify(text)})`), null, text)
  }

  app.context.walkText = JSON.stringify({
    summary: 7,
    steps: [{ n: 1, title: 'A', summary: 'S' }, { n: '2', title: 'B' }, { n: 3 }, null, { n: 4.5, title: 'C' }, { n: 5, title: 'E', summary: 1 }],
  })
  assert.deepEqual(plain(app.run('parseWalk(walkText)')), {
    summary: null,
    steps: [{ n: 1, title: 'A', summary: 'S' }, { n: 5, title: 'E', summary: null }],
  })

  assert.equal(await app.run('loadWalk("api/file/o/r/main/s/", ["session.json"])'), null)
  assert.deepEqual(app.calls, [])
  assert.equal(await app.run('loadWalk("api/file/o/r/main/s/", ["walk.json"])'), null)
  assert.deepEqual(app.calls, ['api/file/o/r/main/s/walk.json'])

  app.context.session = walkSession('walk', 0, 0)
  const stops = plain(app.run('buildStops(session, ["request.md"], [], {}, null)'))
  assert.equal(stops.length, 1)
  assert.equal(stops[0].label, 'Overview')
  assert.equal(stops[0].file, 'overview.md')
  assert.equal(stops[0].state, 'done')
  assert.equal(stops[0].summary, null)
})

test('walk rows read writing n/m', () => {
  const app = dashboard()
  app.context.session = walkSession('walk', 1, 2)

  assert.equal(app.run('stepLabel(session)'), 'walk-through · writing 1/2')
  assert.match(app.run('rowContent(session)'), /<span class="pill">walk-through<\/span> walk-through · writing 1\/2<\/span>/)

  const facts = [...app.run('sessionStripHtml(session, [])').matchAll(/<span class="pill">([^<]*)<\/span>/g)].map((match) => match[1])
  assert.deepEqual(facts, ['walk-through', 'walk', '1/2 steps'])

  app.context.session = walkSession('walk', 1, 2, { counts: undefined })
  assert.equal(app.run('stepLabel(session)'), 'walk-through · writing 0/0')
})

test('a walk opens its overview by default', async () => {
  const api = fileApi({ 'session.json': '{}', 'walk.json': walkJson, 'overview.md': '# Overview\n', 'steps/01.md': '## Step 1: A\n' })
  const app = await openSession(api, walkSession('walk'))

  assert.equal(app.run('defaultFile(session, [], 0)'), 'overview.md')
  assert.equal(app.run('sessionView.selected'), 0)
  assert.deepEqual(plain(app.run('sessionView.stops.map((stop) => stop.label)')), ['Overview', 'Step 1: A', 'Step 2: B'])
  assert.match(app.elements.get('viewer-head').innerHTML, /<span class="path">overview\.md<\/span><span class="by">Overview<\/span>/)
  assert.match(app.elements.get('view').innerHTML, /# Overview/)
})

test('a done review opens its comment by default', async () => {
  const api = fileApi({
    'session.json': '{}',
    'request.md': '# Request\n',
    'round-1/report.md': '# Verify\n',
    'round-1/reviews/independent-review.md': '# R\n',
    'round-1/submit.md': '**[should-fix]**\nA title\n',
  })
  const app = await openSession(api, { ...editSession, flow: 'review', category: 'review', stage: 'done', round: 1, pr: 42 })

  app.context.files = Object.keys(api.files)
  assert.equal(app.run('defaultFile(session, files, sessionRound(session))'), 'round-1/submit.md')

  const selected = plain(app.run('sessionView.stops[sessionView.selected]'))
  assert.equal(selected.kind, 'comment')
  assert.equal(selected.file, 'round-1/submit.md')

  assert.match(app.elements.get('viewer-head').innerHTML, /<span class="path">round-1\/submit\.md<\/span><span class="by">Comment<\/span>/)
  assert.match(app.elements.get('view').innerHTML, /A title/)
  assert.match(app.elements.get('metro').innerHTML, /<div class="card selected" data-path="round-1\/submit\.md">\s*<div class="card-head">\s*<span class="title">Comment<\/span>/)
})

test('moving between walk stops skips unwritten steps', async () => {
  const steps = [1, 2, 3].map((n) => ({ n, title: `S${n}` }))
  const api = fileApi({ 'walk.json': JSON.stringify({ steps }), 'overview.md': '# Overview\n', 'steps/01.md': '## 1\n', 'steps/03.md': '## 3\n' })
  const app = await openSession(api, walkSession('walk', 2, 3))
  const at = () => [app.run('sessionView.selected'), app.context.location.hash]
  const href = (path) => app.run(`fileHref(session, ${JSON.stringify(path)})`)

  assert.ok(press(app, 'ArrowDown'))
  assert.deepEqual(at(), [1, href('steps/01.md')])
  assert.ok(press(app, 'ArrowDown'))
  assert.deepEqual(at(), [3, href('steps/03.md')])
  assert.equal(press(app, 'ArrowDown'), false)
  assert.ok(press(app, 'ArrowUp'))
  assert.deepEqual(at(), [1, href('steps/01.md')])
  assert.ok(press(app, 'ArrowUp'))
  assert.deepEqual(at(), [0, href('overview.md')])
})

// Returns the bytes of a `Uint8Array` from any realm, and any other value unchanged.
const bytes = (value) => (Object.prototype.toString.call(value) === '[object Uint8Array]' ? Array.from(value) : value)

// Gives `app` fake xterm, fit addon, socket and resize observer classes, and returns the instances they create.
function terminalStubs(app) {
  const sockets = []
  const terminals = []
  const observers = []
  const { document } = app.context

  // Records what the page writes and focuses, and keeps the handlers it registers. `size` is what the next fit gives.
  class Terminal {
    cols = 80
    rows = 24
    size = { cols: 100, rows: 30 }
    written = []
    cleared = 0
    focused = false
    disposed = false
    fits = 0
    element = null
    handlers = { data: null, binary: null, resize: null, key: null }

    constructor(options) {
      this.options = options
      terminals.push(this)
    }

    loadAddon(addon) { addon.activate(this) }

    open(container) {
      this.element = document.createElement('div')
      container.replaceChildren(this.element)
    }

    write(data) { this.written.push(data) }
    reset() { this.written = [] }
    clear() { this.cleared += 1 }
    focus() { this.focused = true }
    blur() { this.focused = false }
    dispose() { this.disposed = true }
    onData(handler) { this.handlers.data = handler }
    onBinary(handler) { this.handlers.binary = handler }
    onResize(handler) { this.handlers.resize = handler }
    attachCustomKeyEventHandler(handler) { this.handlers.key = handler }
  }

  // Sets its terminal to the terminal's `size`, and calls the resize handler when the size changed.
  class FitAddon {
    activate(terminal) { this.terminal = terminal }

    fit() {
      const terminal = this.terminal
      const { cols, rows } = terminal.size
      terminal.fits += 1
      if (terminal.cols === cols && terminal.rows === rows) return

      terminal.cols = cols
      terminal.rows = rows
      terminal.handlers.resize?.({ cols, rows })
    }
  }

  // Records its URL, binary type, sent frames and close. A test fires its handlers by hand.
  class WebSocket {
    static OPEN = 1
    readyState = 1
    binaryType = 'blob'
    sent = []
    closed = false

    constructor(url) {
      this.url = url
      sockets.push(this)
    }

    send(data) { this.sent.push(data) }

    close() {
      this.closed = true
      this.readyState = 3
    }
  }

  // Records its target and its callback. A test calls the callback by hand.
  class ResizeObserver {
    target = null
    disconnected = false

    constructor(callback) {
      this.callback = callback
      observers.push(this)
    }

    observe(target) { this.target = target }
    disconnect() { this.disconnected = true }
  }

  app.context.location.host = '127.0.0.1:8731'
  Object.assign(app.context, {
    Terminal,
    FitAddon: { FitAddon },
    WebSocket,
    ResizeObserver,
    TextEncoder,
  })
  return { sockets, terminals, observers }
}

const mainPath = '/s/o/r/main'
const worktreePath = "/s/o/r/work tree's"
const encodedWorktree = "%2Fs%2Fo%2Fr%2Fwork%20tree's"
const worktreeRepos = {
  error: null,
  repos: [
    {
      org: 'o',
      repo: 'r',
      path: mainPath,
      error: null,
      worktrees: [
        { path: mainPath, branch: 'main', head: null },
        { path: worktreePath, branch: 'feature', head: null },
      ],
    },
  ],
}

// The repos of `worktreeRepos` as `api/repos` lists them: without the main checkout among `worktrees`.
const linkedRepos = { error: null, repos: [{ ...worktreeRepos.repos[0], worktrees: [worktreeRepos.repos[0].worktrees[1]] }] }

// Returns the hash of the worktree page of `path`.
const worktreeHashOf = (path) => `#/worktree/${encodeURIComponent(path)}`

// Returns a session `id` of `o/r` in the worktree `worktree`.
const sessionAt = (id, worktree) => ({ ...session, org: 'o', repo: 'r', id, dir: `api/file/o/r/main/${id}/`, worktree })

// Returns a dashboard on the worktree page of `path` after one poll of an api that lists `sessions`, answers `api/repos` with `reposReply()`, `api/terminals` with `terminalsReply()` and each post with `postReply()`, with fake terminal classes, the instances they create and the URLs it posts.
async function worktreeApp(
  path = worktreePath,
  sessions = [],
  reposReply = () => response(JSON.stringify(worktreeRepos)),
  terminalsReply = () => response('', false, { status: 404 }),
  postReply = () => response('', true, { status: 204 }),
) {
  const posts = []
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') {
        posts.push(options.body === undefined ? url : `${url} ${options.body}`)
        return postReply()
      }
      if (url === 'api/repos') return reposReply()
      if (url === 'api/terminals') return terminalsReply()
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
  const stubs = terminalStubs(app)
  app.context.location.hash = worktreeHashOf(path)
  await app.run('poll()')
  return { app, posts, ...stubs }
}

// Goes to `hash` as a click on a link does.
function navigate(app, hash) {
  app.context.location.hash = hash
  app.window.dispatch('hashchange')
}

// Clicks the tab strip button of `action` for the tab `id`.
function clickTab(app, action, id) {
  const button = { dataset: { term: action, tab: String(id) } }
  const target = { closest: (selector) => (selector === 'button[data-term]' ? button : null) }
  app.elements.get('term-tabs').dispatch('click', { target })
}

// Clicks the worktree card button of `action`.
function clickCard(app, action) {
  const button = { dataset: { action } }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('worktree-card').dispatch('click', { target })
}

// Returns the terminal tabs of the worktree at `path`.
const tabsOf = (app, path = worktreePath) => Array.from(app.run(`worktreeTerms.get(${JSON.stringify(path)})?.tabs ?? []`))

// Returns the names in the tab strip, with `*` after the selected tab and `!` after a failed one.
const tabNames = (app) => [...app.elements.get('term-tabs').innerHTML.matchAll(/<span class="term-tab( failed)?">\s*<button type="button" class="term-name" data-term="select" data-tab="\d+" aria-pressed="(true|false)">([^<]*)<\/button>/g)]
  .map((match) => `${match[3]}${match[2] === 'true' ? '*' : ''}${match[1] ? '!' : ''}`)

// Returns the elements in the terminal pane of the worktree page.
const shownTerms = (app) => app.elements.get('term-view').children

// Returns the items of the worktree bar, each with its decoded link and its text.
const barLinks = (app) => [...app.elements.get('worktree-bar').innerHTML.matchAll(/<a class="bar-item[^"]*" href="([^"]*)"[^>]*>([\s\S]*?)<\/a>/g)]
  .map((match) => ({ href: unescapeHtml(match[1]), text: textOf(match[2]) }))

// Returns the decoded links of the worktree bar items that carry `aria-current`.
const currentBarLinks = (app) => [...app.elements.get('worktree-bar').innerHTML.matchAll(/href="([^"]*)" aria-current="page"/g)]
  .map((match) => unescapeHtml(match[1]))

// Returns the element ids in the markup of the shown page.
const pageIds = (app) => [...app.elements.get('page').innerHTML.matchAll(/ id="([^"]+)"/g)].map((match) => match[1])

test('the bar lists the Terminal item, then the sessions of the worktree', async () => {
  const sessions = [
    sessionAt('s1', worktreePath),
    { ...sessionAt('s2', `${worktreePath}/`), flow: 'review', stage: 'verify' },
    sessionAt('s3', mainPath),
  ]
  const { app } = await worktreeApp(worktreePath, sessions)
  const links = [
    { href: worktreeHashOf(worktreePath), text: '▣ Terminal' },
    { href: '#/o/r/main/s1', text: 's1 build implement' },
    { href: '#/o/r/main/s2', text: 's2 review verify' },
  ]

  assert.deepEqual(barLinks(app), links)
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(worktreePath)])

  navigate(app, '#/o/r/main/s2')
  await settle()
  assert.deepEqual(barLinks(app), links)
  assert.deepEqual(currentBarLinks(app), ['#/o/r/main/s2'])

  sessions[0] = { ...sessions[0], stage: 'blocked', blocked: { stage: 'implement' } }
  await app.run('poll()')
  await settle()
  assert.match(app.elements.get('worktree-bar').innerHTML, /<a class="bar-item blocked" href="#\/o\/r\/main\/s1">/)
  assert.match(app.elements.get('worktree-bar').innerHTML, /<a class="bar-item" href="#\/o\/r\/main\/s2" aria-current="page">/)
})

test('the Terminal item shows the tab strip at full width', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])

  assert.deepEqual(pageIds(app), ['worktree-card', 'worktree-bar', 'term-tabs', 'term-view'])
  assert.match(app.elements.get('page').innerHTML, /<div class="session-page terminal-item">/)
  assert.match(app.elements.get('term-tabs').innerHTML, /<button type="button" class="term-add" data-term="add" title="[^"]*">\+<\/button>/)
  assert.match(html, /\.session-page\.terminal-item \{\s*grid-template-columns: minmax\(0, 1fr\);\s*grid-template-rows: auto auto minmax\(0, 1fr\);\s*\}/)
})

test('a session item shows its strip, metro and viewer', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.deepEqual(pageIds(app), ['worktree-card', 'worktree-bar', 'session-card', 'metro-pane', 'metro', 'splitter', 'viewer-head', 'view'])
  assert.match(app.elements.get('page').innerHTML, /<div class="session-page">/)

  const strip = app.elements.get('session-card').innerHTML
  assert.equal(textOf(strip), 'build default implement')
  assert.deepEqual([...strip.matchAll(/data-action="([^"]*)"/g)].map((match) => match[1]), ['archive'])

  const card = app.elements.get('worktree-card').innerHTML
  assert.equal(textOf(card), "o/r o/r/work tree's feature")
  assert.deepEqual([...card.matchAll(/data-action="([^"]*)"/g)].map((match) => match[1]), ['lazygit', 'iterm', 'zed', 'intellij', 'github'])

  for (const part of ['session-bar', 'worktree-bar', 'session-strip']) {
    assert.match(html, new RegExp(`\\.${part} \\{[^}]*grid-column: 1 / -1;`), part)
  }
})

test('switching items parks the terminal and keeps it running', async () => {
  const { app, sockets } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  clickTab(app, 'add')
  const [tab] = tabsOf(app)

  navigate(app, '#/o/r/main/s1')
  await settle()
  assert.deepEqual(app.elements.get('term-store').children, [tab.el])
  assert.equal(sockets[0].closed, false)

  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(shownTerms(app), [tab.el])
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(sockets.length, 1)
})

test('a session on an unlisted worktree renders the page with the reason', async () => {
  const gone = '/s/o/r/gone'
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', gone)])
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.deepEqual(barLinks(app), [
    { href: worktreeHashOf(gone), text: '▣ Terminal' },
    { href: '#/o/r/main/s1', text: 's1 build implement' },
  ])
  assert.equal(textOf(app.elements.get('worktree-card').innerHTML), `${gone} not a configured worktree`)
  assert.ok(pageIds(app).includes('metro'))

  navigate(app, worktreeHashOf(gone))
  assert.deepEqual(pageIds(app), ['worktree-card', 'worktree-bar', 'term-tabs', 'term-view'])
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(gone)])
  assert.equal(textOf(app.elements.get('term-tabs').innerHTML), 'not a configured worktree')
})

test('a session without a worktree renders its page', async () => {
  for (const shown of [session, { ...session, worktree: '' }]) {
    const other = { ...shown, org: 'other', id: 'other', dir: 'api/file/other/repo/main/other/' }
    const { app } = await worktreeApp(worktreePath, [shown, other])
    navigate(app, '#/org/repo/main/session')
    await settle()

    assert.deepEqual(barLinks(app), [{ href: '#/org/repo/main/session', text: 'session build implement' }])
    assert.deepEqual(currentBarLinks(app), ['#/org/repo/main/session'])
    assert.doesNotMatch(app.elements.get('worktree-bar').innerHTML, /#\/worktree\//)
    assert.ok(pageIds(app).includes('metro'))

    for (const hash of ['#/org/repo/main/session', '#/org/repo/main/session/spec.md']) {
      navigate(app, hash)
      await settle()
      assert.ok(pressBar(app, 'next'))
      assert.ok(pressBar(app, 'previous'))
      assert.equal(app.context.location.hash, hash)
    }

    await archive(app, shown)
    assert.equal(app.context.location.hash, '#/sessions')
  }
})

test('switching items with unsaved changes asks to discard them', async () => {
  const app = dashboard({ fetcher: fileApi({ 'spec.md': '# Spec\n' }).fetcher })
  terminalStubs(app)
  app.context.session = { ...editSession, worktree: worktreePath }
  app.run('state.sessions = [session]')
  const href = app.run('fileHref(session, "spec.md")')
  navigate(app, href)
  await settle()

  press(app, 'w')
  editor(app).value = '# Typed\n'
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, href)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.ok(pageIds(app).includes('view'))
})

test('the worktree buttons show their keys', async () => {
  const { app } = await worktreeApp()

  const card = app.elements.get('worktree-card').innerHTML
  assert.deepEqual([...card.matchAll(/data-action="([^"]*)" title="([^"]*)"/g)].map((match) => `${match[1]}: ${match[2]}`), [
    'lazygit: Open lazygit in a tab (g)',
    'iterm: Open in iTerm',
    'zed: Open in Zed (e)',
    'intellij: Open in IntelliJ (i)',
    'github: Open on GitHub (o)',
  ])
  assert.match(app.elements.get('term-tabs').innerHTML, /<button type="button" class="term-add" data-term="add" title="Open a shell \(t\)">\+<\/button>/)
})

test('plus opens a shell tab on the terminal route', async () => {
  const { app, sockets, terminals } = await worktreeApp()

  clickTab(app, 'add')
  const [first] = tabsOf(app)
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&name=shell%201&cols=100&rows=30`)
  assert.equal(sockets[0].binaryType, 'arraybuffer')
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(terminals[0].element.parentElement, first.el)
  assert.equal(terminals[0].focused, true)
  assert.equal(terminals[0].options.theme, app.run('TERMINAL_THEME'))

  clickTab(app, 'add')
  const [, second] = tabsOf(app)
  assert.equal(sockets[1].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&name=shell%202&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])
  assert.equal(first.el.parentElement, app.elements.get('term-store'))

  const view = app.elements.get('term-view')
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view, 'a poll keeps the page')
  assert.deepEqual(shownTerms(app), [second.el])
})

test('the lazygit button opens a lazygit tab', async () => {
  const { app, sockets } = await worktreeApp()

  clickCard(app, 'lazygit')
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=lazygit&name=lazygit&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['lazygit*'])

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['lazygit', 'shell 1*'])
})

test('closing a tab closes its socket', async () => {
  const { app, sockets, terminals, observers } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)

  clickTab(app, 'close', second.id)
  assert.equal(sockets[1].closed, true)
  assert.equal(sockets[1].onclose, null)
  assert.equal(terminals[1].disposed, true)
  assert.equal(observers[1].disconnected, true)
  assert.equal(second.el.parentElement, null)
  assert.equal(sockets[0].closed, false)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(sockets[0].closed, true)
  assert.deepEqual(tabNames(app), [])
  assert.deepEqual(shownTerms(app), [])

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['shell 3*'])
})

test('program exit closes its tab', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first] = tabsOf(app)

  sockets[1].onclose({ code: 1000, reason: '' })
  assert.equal(terminals[1].disposed, true)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(app.elements.get('modal').open, undefined)
})

test('a failed start keeps the tab with the reason', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const [first] = tabsOf(app)

  sockets[0].onclose({ code: 1011, reason: 'not a configured worktree' })
  assert.equal(first.failed, 'not a configured worktree')
  assert.equal(terminals[0].disposed, false)
  assert.ok(terminals[0].written.some((data) => String(data).includes('not a configured worktree')), terminals[0].written.join())
  assert.deepEqual(tabNames(app), ['shell 1*!'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(app.elements.get('modal').open, undefined)

  clickTab(app, 'add')
  sockets[1].onclose({ code: 1006, reason: '' })
  assert.equal(tabsOf(app)[1].failed, 'the connection closed')

  clickTab(app, 'close', first.id)
  assert.deepEqual(tabNames(app), ['shell 2*!'])
})

test('terminals survive leaving and returning', async () => {
  const at = { ...session, org: 'o', repo: 'r', id: 's1', dir: 'api/file/o/r/main/s1/', worktree: worktreePath }
  const { app, sockets, terminals } = await worktreeApp(worktreePath, [at])
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)
  clickTab(app, 'select', first.id)
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])

  navigate(app, '#/o/r/main/s1')
  await settle()
  assert.match(app.elements.get('page').innerHTML, /id="metro-pane"/)
  assert.deepEqual(new Set(app.elements.get('term-store').children), new Set([first.el, second.el]))

  app.run('render()')
  await settle()
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(terminals.length, 2)
  assert.deepEqual(tabsOf(app).map((tab) => tab.term), terminals)
  assert.deepEqual(sockets.map((socket) => socket.closed), [false, false])
  assert.deepEqual(terminals.map((terminal) => terminal.disposed), [false, false])
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.deepEqual(app.elements.get('term-store').children, [second.el])
})

test('each worktree keeps its own tabs', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const own = tabsOf(app)

  navigate(app, worktreeHashOf(mainPath))
  assert.deepEqual(tabNames(app), [])
  assert.deepEqual(shownTerms(app), [])

  clickTab(app, 'add')
  const [other] = tabsOf(app, mainPath)
  assert.equal(sockets[2].url, 'ws://127.0.0.1:8731/api/terminal?path=%2Fs%2Fo%2Fr%2Fmain&program=shell&name=shell%201&cols=100&rows=30')
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [other.el])

  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [own[1].el])
  assert.equal(other.el.parentElement, app.elements.get('term-store'))
  assert.deepEqual(sockets.map((socket) => socket.closed), [false, false, false])
})

test('the repos page counts open terminals', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  navigate(app, '#/')
  assert.deepEqual(worktreeRows(app).map((row) => row.text), [
    'main o/r/main 0 sessions',
    "feature o/r/work tree's 0 sessions · ▣ 2",
  ])

  sockets[0].onclose({ code: 1000, reason: '' })
  assert.equal(worktreeRows(app)[1].text, "feature o/r/work tree's 0 sessions · ▣ 1")
})

// Returns a reply to `api/terminals` that lists `terminals`.
const terminalsReply = (terminals) => response(JSON.stringify({ terminals }))

// Returns `worktreeApp()` whose server runs the terminals the array `running` holds at each poll.
const runningApp = (running) => worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))

const shellOnServer = { id: 7, path: worktreePath, program: 'shell', name: 'shell 4' }
const claudeOnServer = { id: 8, path: worktreePath, program: 'claude', name: 'claude' }
const mainOnServer = { id: 9, path: mainPath, program: 'lazygit', name: 'lazygit' }

test('the hello frame gives a tab its server id, and closing the tab kills its terminal', async () => {
  const { app, posts, sockets } = await worktreeApp()
  clickTab(app, 'add')
  const [tab] = tabsOf(app)

  sockets[0].onmessage({ data: '{"id":7}' })
  assert.equal(tab.serverId, 7)

  clickTab(app, 'close', tab.id)
  await settle()
  assert.deepEqual(posts, ['api/terminals/7/kill'])
})

test('program exit or a tab without a server id posts no kill', async () => {
  const { app, posts, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  sockets[1].onmessage({ data: '{"id":7}' })

  sockets[1].onclose({ code: 1000, reason: '' })
  clickTab(app, 'close', tabsOf(app)[0].id)
  await settle()
  assert.deepEqual(posts, [])
})

test('a poll adds a tab for each running terminal and connects only the shown one', async () => {
  const { app, sockets } = await runningApp([shellOnServer, claudeOnServer, mainOnServer])

  assert.deepEqual(tabNames(app), ['shell 4*', 'claude'])
  assert.deepEqual(sockets.map((socket) => socket.url), ['ws://127.0.0.1:8731/api/terminal?id=7&cols=100&rows=30'])

  const [, claude] = tabsOf(app)
  clickTab(app, 'select', claude.id)
  assert.equal(sockets[1].url, 'ws://127.0.0.1:8731/api/terminal?id=8&cols=100&rows=30')
  assert.equal(sockets[0].closed, false)

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['shell 4', 'claude', 'shell 5*'])

  navigate(app, worktreeHashOf(mainPath))
  assert.deepEqual(tabNames(app), ['lazygit*'])
  assert.equal(sockets.at(-1).url, 'ws://127.0.0.1:8731/api/terminal?id=9&cols=100&rows=30')
})

test('a terminal that ended elsewhere leaves its tab without a kill', async () => {
  let running = [shellOnServer, claudeOnServer]
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  const [, claude] = tabsOf(app)

  running = [shellOnServer]
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['shell 4*'])
  assert.equal(claude.term.disposed, true)
  assert.deepEqual(posts, [])
})

test('a poll takes the names the server holds', async () => {
  let running = [shellOnServer]
  const { app } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))

  running = [{ ...shellOnServer, name: 'build' }]
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['build*'])
})

test('a poll adds no tab to a worktree whose new tab waits for its server id', async () => {
  let running = []
  const { app, sockets } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  clickTab(app, 'add')

  running = [shellOnServer]
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['shell 1*'])

  sockets[0].onmessage({ data: JSON.stringify({ id: shellOnServer.id }) })
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['shell 4*'])
})

test('a take-over leaves the tab to the other window until the tab is selected again', async () => {
  const { app, sockets, terminals } = await runningApp([shellOnServer])
  const [tab] = tabsOf(app)
  terminals[0].write('old output')

  sockets[0].onclose({ code: 4001, reason: 'attached in another window' })
  assert.equal(tab.away, true)
  assert.ok(terminals[0].written.some((data) => String(data).includes('Attached in another window')), terminals[0].written.join())
  assert.equal(tab.failed, null)

  await app.run('poll()')
  assert.equal(sockets.length, 1, 'a poll leaves the terminal to the other window')

  clickTab(app, 'select', tab.id)
  assert.equal(sockets.length, 2)
  assert.equal(sockets[1].url, 'ws://127.0.0.1:8731/api/terminal?id=7&cols=100&rows=30')
  assert.deepEqual(terminals[0].written, [], 'the replay starts on a clean terminal')
})

test('the replay and the live output reach the terminal of an attached tab', async () => {
  const { sockets, terminals } = await runningApp([shellOnServer])

  sockets[0].onmessage({ data: new TextEncoder().encode('kept').buffer })
  assert.deepEqual(terminals[0].written.map((data) => Buffer.from(data).toString()), ['kept'])
})

test('a rename posts the new name for a tab with a server id', async () => {
  const { app, posts } = await runningApp([shellOnServer])

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = 'build'
  pressInRename(app, 'Enter')
  await settle()
  assert.deepEqual(posts, ['api/terminals/7/name build'])
})

test('a shell tab shows the command it runs until a user names it', async () => {
  let running = [{ ...shellOnServer, command: 'claude' }, { ...claudeOnServer, command: 'node' }]
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  assert.deepEqual(tabNames(app), ['claude*', 'claude'])

  running = [{ ...shellOnServer, command: null }, claudeOnServer]
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['shell 4*', 'claude'])

  running = [{ ...shellOnServer, name: 'build', renamed: true, command: 'vim' }, claudeOnServer]
  await app.run('poll()')
  assert.deepEqual(tabNames(app), ['build*', 'claude'])

  press(app, 'i', cmd)
  pressInRename(app, 'Enter')
  await settle()
  assert.deepEqual(posts, [], 'an unchanged name posts nothing')
})

test('a polled terminal state reaches its tab', async () => {
  let running = [{ ...shellOnServer, state: 'idle' }]
  const { app } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  const [tab] = tabsOf(app)
  assert.equal(tab.state, 'idle')

  running = [{ ...shellOnServer, state: 'asleep' }]
  await app.run('poll()')
  assert.equal(tab.state, 'busy', 'an unknown state is busy')

  clickTab(app, 'add')
  assert.equal(tabsOf(app)[1].state, 'busy', 'a new tab is busy')
})

test('a shown tab in attention posts seen and turns idle', async () => {
  const { app, posts } = await runningApp([{ ...shellOnServer, state: 'attention' }])
  await settle()

  assert.deepEqual(posts, ['api/terminals/7/seen'])
  assert.equal(tabsOf(app)[0].state, 'idle')
})

test('a hidden tab in attention stays and posts nothing', async () => {
  let running = []
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  navigate(app, '#/')

  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(tabsOf(app)[0].state, 'attention')
})

test("an active tab in attention on another worktree's page stays and posts nothing", async () => {
  const { app, posts } = await runningApp([shellOnServer, { ...mainOnServer, state: 'attention' }])
  await settle()

  const [lazygit] = tabsOf(app, mainPath)
  assert.equal(app.run(`worktreeTerms.get(${JSON.stringify(mainPath)}).active`), lazygit.id)
  assert.deepEqual(posts, [])
  assert.equal(lazygit.state, 'attention')
})

test('a shown tab in an unfocused window stays and posts nothing', async () => {
  let running = []
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  app.context.document.focused = false

  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(tabsOf(app)[0].state, 'attention')
})

test('a shown tab in a hidden document stays and posts nothing', async () => {
  let running = []
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  app.context.document.visibilityState = 'hidden'

  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(tabsOf(app)[0].state, 'attention')
})

test('an away tab in attention stays and posts nothing', async () => {
  let running = [shellOnServer]
  const { app, posts, sockets } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  sockets[0].onclose({ code: 4001, reason: 'attached in another window' })

  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(tabsOf(app)[0].state, 'attention')
})

test('a failed tab in attention stays and posts nothing', async () => {
  let running = [shellOnServer]
  const { app, posts, sockets } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  sockets[0].onclose({ code: 1006, reason: '' })

  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(tabsOf(app)[0].state, 'attention')
})

test('selecting a tab in attention posts seen', async () => {
  const { app, posts } = await runningApp([shellOnServer, { ...claudeOnServer, state: 'attention' }])
  const [, claude] = tabsOf(app)
  await settle()
  assert.deepEqual(posts, [], 'a tab that is not active stays')

  clickTab(app, 'select', claude.id)
  await settle()
  assert.deepEqual(posts, ['api/terminals/8/seen'])
  assert.equal(claude.state, 'idle')
})

test('opening the worktree page posts seen for its active tab in attention', async () => {
  let running = []
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  navigate(app, '#/')
  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')

  navigate(app, worktreeHashOf(worktreePath))
  await settle()
  assert.deepEqual(posts, ['api/terminals/7/seen'])
  assert.equal(tabsOf(app)[0].state, 'idle')
})

test('the window focus posts seen for the shown tab in attention', async () => {
  let running = []
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  app.context.document.focused = false
  running = [{ ...shellOnServer, state: 'attention' }]
  await app.run('poll()')

  app.context.document.focused = true
  app.window.dispatch('focus')
  await settle()
  assert.deepEqual(posts, ['api/terminals/7/seen'])
  assert.equal(tabsOf(app)[0].state, 'idle')
})

test('a seen post that gets 404 tells nothing', async () => {
  const running = () => terminalsReply([{ ...shellOnServer, state: 'attention' }])
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, running, () => response('no such terminal', false, { status: 404 }))
  await settle()

  assert.deepEqual(posts, ['api/terminals/7/seen'])
  assert.notEqual(app.elements.get('modal').open, true)
})

test('a seen post that fails tells nothing', async () => {
  const running = () => terminalsReply([{ ...shellOnServer, state: 'attention' }])
  const { app, posts } = await worktreeApp(worktreePath, [], undefined, running, () => response('boom', false))
  await settle()

  assert.deepEqual(posts, ['api/terminals/7/seen'])
  assert.notEqual(app.elements.get('modal').open, true)
})

const codexOnServer = { id: 10, path: worktreePath, program: 'codex', name: 'codex' }

// Returns the state of the worktree at `path`.
const worktreeStateOf = (app, path = worktreePath) => app.run(`worktreeState(${JSON.stringify(path)})`)

test('the worktree state takes attention over busy over idle', async () => {
  let running = [{ ...shellOnServer, state: 'idle' }, { ...claudeOnServer, state: 'busy' }]
  const { app } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  navigate(app, '#/')
  assert.equal(worktreeStateOf(app), 'busy')
  assert.equal(app.run('anyAttention()'), false)

  running = [...running, { ...codexOnServer, state: 'attention' }]
  await app.run('poll()')
  assert.equal(worktreeStateOf(app), 'attention')
  assert.equal(app.run('anyAttention()'), true)

  running = [{ ...shellOnServer, state: 'idle' }, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.equal(worktreeStateOf(app), 'idle')
  assert.equal(app.run('anyAttention()'), false)

  running = []
  await app.run('poll()')
  assert.equal(worktreeStateOf(app), null)
  assert.equal(worktreeStateOf(app, mainPath), null)
})

test('the repos row colors its square by worktree state', async () => {
  let running = [shellOnServer, { ...claudeOnServer, state: 'attention' }]
  const { app } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  const list = () => app.elements.get('repo-list').innerHTML
  navigate(app, '#/')

  assert.match(list(), /<span class="term-attention" title="needs you">▣ 2<\/span>/)
  assert.equal(worktreeRows(app)[1].text, "feature o/r/work tree's 0 sessions · ▣ 2")

  running = [{ ...shellOnServer, state: 'idle' }, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.match(list(), /<span class="term-idle" title="idle">▣ 2<\/span>/)

  running = [shellOnServer, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.match(list(), /<span class="term-busy" title="busy">▣ 2<\/span>/)

  assert.match(html, /\.term-attention \{ color: var\(--critical\); \}/)
  assert.match(html, /\.term-idle \{ color: var\(--accent\); \}/)
  assert.match(html, /\.term-busy \{ color: var\(--ink-2\); \}/)
})

// Returns each tab of the tab strip as its name and the markup of its dot, or null.
const tabDots = (app) => [...app.elements.get('term-tabs').innerHTML.matchAll(/class="term-name"[^>]*>(<span class="term-dot[^"]*"[^>]*>●<\/span>)?([^<]*)<\/button>/g)]
  .map((match) => [match[2], match[1] ?? null])

test('the tab strip shows a dot for attention and idle, none for busy', async () => {
  const { app } = await runningApp([shellOnServer, { ...claudeOnServer, state: 'attention' }, { ...codexOnServer, state: 'idle' }])

  assert.deepEqual(tabDots(app), [
    ['shell 4', null],
    ['claude', '<span class="term-dot term-attention" title="needs you">●</span>'],
    ['codex', '<span class="term-dot term-idle" title="idle">●</span>'],
  ])
})

test('Shift+Enter sends Escape and Return to a worktree terminal', async () => {
  const { sockets, terminals } = await runningApp([shellOnServer])
  const key = (type, modifiers) => ({ type, key: 'Enter', shiftKey: true, altKey: false, ctrlKey: false, metaKey: false, ...modifiers })

  assert.equal(terminals[0].handlers.key(key('keydown')), false)
  assert.equal(terminals[0].handlers.key(key('keypress')), false)
  assert.deepEqual(sockets[0].sent.map(bytes), [[27, 13]])

  assert.equal(terminals[0].handlers.key(key('keydown', { shiftKey: false })), true)
  assert.equal(sockets[0].sent.length, 1)
})

test('the terminals page lists the tabs of every worktree', async () => {
  const { app } = await runningApp([shellOnServer, claudeOnServer, mainOnServer])
  const away = tabsOf(app)[1]
  away.away = true

  navigate(app, '#/terminals')
  const page = app.elements.get('page').innerHTML
  assert.deepEqual([...page.matchAll(/<h2 class="org">([\s\S]*?)<\/h2>/g)].map((match) => textOf(match[1])), [
    "feature o/r/work tree's",
    'main o/r/main',
  ])
  assert.deepEqual([...page.matchAll(/<a class="body" href="([^"]*)"[\s\S]*?<\/a>/g)].map((match) => `${unescapeHtml(match[1])} ${textOf(match[0])}`), [
    `${worktreeHashOf(worktreePath)} shell 4 shell`,
    `${worktreeHashOf(worktreePath)} claude claude · in another window`,
    `${worktreeHashOf(mainPath)} lazygit lazygit`,
  ])
})

test('the terminals page labels attention and idle rows', async () => {
  const { app } = await runningApp([{ ...shellOnServer, state: 'idle' }, { ...claudeOnServer, state: 'attention' }, mainOnServer])

  navigate(app, '#/terminals')
  const page = app.elements.get('page').innerHTML
  assert.deepEqual([...page.matchAll(/<span class="label">([\s\S]*?)<\/span>\s*<\/a>/g)].map((match) => unescapeHtml(match[1])), [
    '<span class="term-idle" title="idle">shell · idle</span>',
    '<span class="term-attention" title="needs you">claude · needs you</span>',
    'lazygit',
  ])
  assert.deepEqual([...page.matchAll(/<h2 class="org">([\s\S]*?)<\/h2>/g)].map((match) => unescapeHtml(match[1])), [
    `<span class="term-attention" title="needs you">feature o/r/work tree's</span>`,
    '<span class="term-busy" title="busy">main o/r/main</span>',
  ])
})

// Returns the markup of each row label of the terminals page.
const terminalLabels = (app) => [...app.elements.get('page').innerHTML.matchAll(/<span class="label">([\s\S]*?)<\/span>\s*<\/a>/g)]
  .map((match) => unescapeHtml(match[1]))

test('the terminals page labels an away tab in attention', async () => {
  const { app } = await runningApp([shellOnServer, { ...claudeOnServer, state: 'attention' }])
  tabsOf(app)[1].away = true

  navigate(app, '#/terminals')
  assert.deepEqual(terminalLabels(app), [
    'shell',
    '<span class="term-attention" title="needs you">claude · in another window · needs you</span>',
  ])
})

test('the terminals page labels an idle failed tab', async () => {
  const { app } = await runningApp([{ ...shellOnServer, state: 'idle' }, claudeOnServer])
  tabsOf(app)[0].failed = 'the connection closed'

  navigate(app, '#/terminals')
  assert.deepEqual(terminalLabels(app), [
    '<span class="term-idle" title="idle">the connection closed · idle</span>',
    'claude',
  ])
})

test('arrow keys and Enter open a tab of the terminals page on its worktree page', async () => {
  const { app, sockets } = await runningApp([shellOnServer, claudeOnServer, mainOnServer])
  const [, claude] = tabsOf(app)
  claude.away = true
  navigate(app, '#/terminals')

  assert.ok(press(app, 'ArrowDown'))
  assert.ok(press(app, 'ArrowDown'))
  assert.match(app.elements.get('page').innerHTML, /class="row worktree selected">\s*<a class="body"[^>]*data-tab="2"/)

  assert.ok(press(app, 'Enter'))
  app.window.dispatch('hashchange')
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 4', 'claude*'])
  assert.equal(claude.away, false)
  assert.equal(sockets.at(-1).url, 'ws://127.0.0.1:8731/api/terminal?id=8&cols=100&rows=30')
})

test('the terminals page says when no terminal runs', async () => {
  const { app } = await worktreeApp()

  navigate(app, '#/terminals')
  assert.equal(pageText(app), 'No terminals run.')
  assert.equal(press(app, 'Enter'), false)
})

test('card buttons post to the worktree open route', async () => {
  const { app, posts, sockets } = await worktreeApp()

  for (const tool of ['zed', 'iterm', 'intellij', 'github']) clickCard(app, tool)
  await settle()
  assert.deepEqual(posts, ['zed', 'iterm', 'intellij', 'github'].map((tool) => `api/open/${tool}?path=${encodedWorktree}`))
  assert.equal(sockets.length, 0)
  assert.equal(app.elements.get('modal').open, undefined)
})

test('an unknown worktree path says so', async () => {
  const { app } = await worktreeApp('/s/o/r')
  assert.equal(pageText(app), 'not a configured worktree')
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)

  const loading = dashboard()
  loading.context.location.hash = worktreeHashOf(worktreePath)
  loading.run('render()')
  assert.equal(pageText(loading), 'loading…')
})

test('a worktree page without tabs shows the repos error', async () => {
  const { app } = await worktreeApp(worktreePath, [], () => response('', false, { status: 500 }))
  assert.equal(pageText(app), '500 api/repos')
  assert.match(app.elements.get('page').innerHTML, /class="config-error"/)
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)
})

test('a config error keeps the terminal tabs and shows the error', async () => {
  const error = 'Cannot read the repos from /h/.jam-factory/config.json: expected value'
  let repos = worktreeRepos
  const { app, sockets, terminals } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(repos)))
  clickTab(app, 'add')
  const [first] = tabsOf(app)
  const view = app.elements.get('term-view')
  const card = () => app.elements.get('worktree-card').innerHTML

  repos = { error, repos: [] }
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(sockets[0].closed, false)
  assert.equal(textOf(card()), `/s/o/r/work tree's ${error}`)
  assert.match(card(), /class="config-error"/)
  assert.doesNotMatch(card(), /data-action/)

  repos = worktreeRepos
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view)
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(terminals[0].focused, true)
  assert.equal(sockets[0].closed, false)
  assert.equal(textOf(card()), "o/r o/r/work tree's feature")
})

test('a removed worktree keeps its open tabs reachable and closable', async () => {
  const [main] = worktreeRepos.repos[0].worktrees
  let repos = worktreeRepos
  const { app, sockets } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(repos)))
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)

  repos = { error: null, repos: [{ ...worktreeRepos.repos[0], worktrees: [main] }] }
  await app.run('poll()')
  assert.equal(textOf(app.elements.get('worktree-card').innerHTML), "/s/o/r/work tree's not a configured worktree")
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])

  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])

  clickTab(app, 'select', first.id)
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(sockets[0].closed, true)
  assert.deepEqual(tabNames(app), ['shell 2*'])

  clickTab(app, 'close', second.id)
  assert.equal(sockets[1].closed, true)
  assert.equal(pageText(app), 'not a configured worktree')
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)
})

test('keys inside a worktree terminal fire no board shortcut', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const hash = app.context.location.hash

  assert.equal(press(app, 'l', {}, terminals[0].element), false)
  assert.equal(press(app, 'Escape', {}, terminals[0].element), false)
  assert.equal(app.context.location.hash, hash)

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
})

test('a font size change resizes the worktree terminals', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')

  terminals[0].size = { cols: 120, rows: 36 }
  app.run('stepFontSize(1)')
  assert.equal(terminals[0].options.fontSize, Math.round((18 + 1) * 0.8))
  assert.deepEqual(sockets[0].sent, ['{"cols":120,"rows":36}'])
})

test('Cmd+= and Cmd+- step the terminal font size on the Terminal item and store it', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))

  assert.ok(press(app, '=', { metaKey: true }, terminals[0].element))
  assert.ok(press(app, '=', { metaKey: true }))
  assert.equal(terminals[0].options.fontSize, Math.round(18 * 0.8) + 2)
  assert.equal(app.run('fontSize'), 18)

  assert.ok(press(app, '-', { metaKey: true }))
  assert.equal(terminals[0].options.fontSize, Math.round(18 * 0.8) + 1)
  assert.equal(app.run("localStorage.getItem('jam-board-terminal-font-size')"), String(Math.round(18 * 0.8) + 1))
})

test('a stored terminal font size outlives a board font size change', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  app.run('stepTerminalSize(1)')

  app.run('stepFontSize(2)')
  assert.equal(terminals[0].options.fontSize, Math.round(18 * 0.8) + 1)
})

test('Cmd+= and Cmd+- step the viewer zoom on a session and store it', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  navigate(app, '#/o/r/main/s1')
  await settle()
  const zoom = () => app.context.document.documentElement.style['--viewer-zoom']
  assert.equal(zoom(), '1')

  assert.ok(press(app, '+', { metaKey: true, shiftKey: true }))
  assert.equal(zoom(), '1.1')
  assert.ok(press(app, '-', { metaKey: true }))
  assert.ok(press(app, '-', { metaKey: true }))
  assert.equal(zoom(), '0.9')
  assert.equal(app.run('fontSize'), 18)
  assert.equal(app.run("localStorage.getItem('jam-board-viewer-zoom')"), '0.9')
})

test('Cmd+= steps the board font size on the board pages', () => {
  for (const hash of ['#/', '#/sessions', '#/terminals']) {
    const storage = new Map()
    const app = dashboard({ hash, storage })
    assert.ok(press(app, '=', { metaKey: true }))
    assert.equal(app.run('fontSize'), 19)
    assert.equal(storage.get('jam-board-font-size'), '19')
  }
})

test('the stored tab text sizes come back when the board opens', () => {
  const storage = new Map([['jam-board-terminal-font-size', '20'], ['jam-board-viewer-zoom', '1.25'], ['jam-board-github-zoom', '0.8']])
  const app = dashboard({ storage })

  assert.equal(app.run('terminalFontSize()'), 20)
  assert.equal(app.context.document.documentElement.style['--viewer-zoom'], '1.25')
  assert.equal(app.run('githubZoom'), 0.8)
})

test('stored tab text sizes out of range fall back to the defaults', () => {
  const storage = new Map([['jam-board-terminal-font-size', '99'], ['jam-board-viewer-zoom', '1.3'], ['jam-board-github-zoom', 'big']])
  const app = dashboard({ storage })

  assert.equal(app.run('terminalFontSize()'), Math.round(18 * 0.8))
  assert.equal(app.context.document.documentElement.style['--viewer-zoom'], '1')
  assert.equal(app.run('githubZoom'), 1)
})

test('the zoom stops at the first and the last level', () => {
  const app = dashboard()
  assert.equal(app.run('stepZoom(2, 1)'), 2)
  assert.equal(app.run('stepZoom(0.5, -1)'), 0.5)
  assert.equal(app.run('stepZoom(1, 1)'), 1.1)
})

test('the app leaves Cmd+= and Cmd+- to its menu', () => {
  const app = dashboard({ tauri: { core: { invoke: async () => {} } } })

  assert.equal(press(app, '=', { metaKey: true }), false)
  assert.equal(press(app, '-', { metaKey: true }), false)
  assert.equal(app.run('fontSize'), 18)
})

test('worktree terminals keep the text size keys', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  assert.equal(key(event('=')), false)
  assert.equal(key(event('+', { shiftKey: true })), false)
  assert.equal(key(event('-')), false)
  assert.equal(key(event('-', { metaKey: false })), true)
})

const zoomKeys = { metaKey: true, shiftKey: true }

test('Shift+Cmd+K zooms the worktree terminal in and Shift+Cmd+J zooms it out', async () => {
  for (const [zoomIn, zoomOut] of [['K', 'J'], ['k', 'j']]) {
    for (const inside of [true, false]) {
      const { app, terminals } = await worktreeApp()
      clickTab(app, 'add')
      const [xterm] = terminals
      const page = app.elements.get('session-page')
      const target = inside ? xterm.element : undefined
      const fits = xterm.fits

      xterm.blur()
      assert.ok(press(app, zoomIn, zoomKeys, target), `${zoomIn} is taken`)
      assert.equal(page.classList.contains('zoomed'), true)
      assert.equal(xterm.fits, fits + 1)
      assert.equal(xterm.focused, true)

      xterm.blur()
      assert.ok(press(app, zoomOut, zoomKeys, target), `${zoomOut} is taken`)
      assert.equal(page.classList.contains('zoomed'), false)
      assert.equal(xterm.fits, fits + 2)
      assert.equal(xterm.focused, true)
    }
  }
})

test('the zoom keys do nothing on a worktree page without tabs', async () => {
  const { app } = await worktreeApp()
  const page = app.elements.get('session-page')

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(page.classList.contains('zoomed'), false)
})

test('closing a tab keeps the zoom until the last one', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)
  const page = app.elements.get('session-page')
  assert.ok(press(app, 'K', zoomKeys))

  clickTab(app, 'close', second.id)
  assert.equal(page.classList.contains('zoomed'), true)
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(page.classList.contains('zoomed'), false)
})

test('worktree terminals keep the zoom keys', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  for (const name of ['K', 'k', 'J', 'j']) assert.equal(key(event(name, zoomKeys)), false, `${name} stays out of the terminal`)
  assert.equal(key(event('k')), true)
})

test('leaving the worktree page drops the zoom', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')
  assert.ok(press(app, 'K', zoomKeys))
  assert.equal(app.elements.get('session-page').classList.contains('zoomed'), true)

  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(app.elements.get('session-page').classList.contains('zoomed'), false)
})

test('the zoom keys leave a parked worktree terminal alone on another page', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  navigate(app, '#/')
  terminals[0].blur()

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(terminals[0].focused, false)
})

test('zoom acts only on the Terminal item', async () => {
  const { app, terminals } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  clickTab(app, 'add')
  navigate(app, '#/o/r/main/s1')
  await settle()
  terminals[0].blur()

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(app.elements.get('session-page').classList.contains('zoomed'), false)
  assert.equal(terminals[0].focused, false)
})

test('a window resize on the Terminal item is safe', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])

  assert.doesNotThrow(() => app.window.dispatch('resize'))
  assert.equal(app.elements.get('session-page').style.getPropertyValue('--left'), '')
})

const tabKeyModifiers = { metaKey: true, shiftKey: true }
const cmd = { metaKey: true }

// Presses `key` in the tab name field and returns true when the page took it.
function pressInRename(app, key) {
  let prevented = false
  app.elements.get('term-tabs').dispatch('keydown', {
    key,
    target: app.elements.get('term-rename'),
    stopPropagation: () => {},
    preventDefault: () => { prevented = true },
  })
  return prevented
}

test('Shift+Cmd+[ and Shift+Cmd+] move through the tabs and wrap around', async () => {
  const { app, terminals } = await worktreeApp()
  for (let i = 0; i < 3; i += 1) clickTab(app, 'add')
  const tabs = tabsOf(app)
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])

  assert.ok(press(app, '}', tabKeyModifiers, terminals[2].element))
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2', 'shell 3'])
  assert.deepEqual(shownTerms(app), [tabs[0].el])
  assert.equal(terminals[0].focused, true)

  assert.ok(press(app, '{', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])

  assert.ok(press(app, '[', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*', 'shell 3'])

  assert.ok(press(app, ']', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])
})

test('Cmd+T opens a shell tab', async () => {
  const { app, sockets, terminals } = await worktreeApp()

  assert.ok(press(app, 't', cmd))
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&name=shell%201&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1*'])

  assert.ok(press(app, 't', cmd, terminals[0].element))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
})

test('Cmd+K clears the active terminal', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  terminals[1].blur()

  assert.ok(press(app, 'k', cmd))
  assert.deepEqual(terminals.map((terminal) => terminal.cleared), [0, 1])
  assert.equal(terminals[1].focused, true)

  assert.ok(press(app, 'K', cmd, terminals[1].element))
  assert.deepEqual(terminals.map((terminal) => terminal.cleared), [0, 2])
  assert.equal(app.run('jump'), null)
})

test('the terminal leaves Cmd+K to the board', async () => {
  const { terminals } = await runningApp([shellOnServer])
  const key = (modifiers) => ({ type: 'keydown', key: 'k', metaKey: true, shiftKey: false, altKey: false, ctrlKey: false, ...modifiers })

  assert.equal(terminals[0].handlers.key(key()), false)
  assert.equal(terminals[0].handlers.key(key({ metaKey: false })), true)
})

test('Cmd+K does nothing without tabs, off the Terminal item or under the modal', async () => {
  const { app, terminals } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  assert.equal(press(app, 'k', cmd), false)

  clickTab(app, 'add')
  app.elements.get('modal').open = true
  assert.equal(press(app, 'k', cmd), false)
  app.elements.get('modal').open = false

  navigate(app, '#/o/r/main/s1')
  await settle()
  assert.equal(press(app, 'k', cmd), false)
  assert.equal(terminals[0].cleared, 0)
})

test('the tab keys do nothing without tabs or off the worktree page', async () => {
  const { app, sockets } = await worktreeApp()

  assert.equal(press(app, ']', tabKeyModifiers), false)
  assert.equal(press(app, 'i', cmd), false)
  assert.equal(app.elements.get('term-rename'), undefined)

  navigate(app, '#/')
  assert.equal(press(app, 't', cmd), false)
  assert.equal(sockets.length, 0)
})

test('the tab keys do nothing on a session item', async () => {
  const { app, sockets } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  clickTab(app, 'add')
  clickTab(app, 'add')
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.equal(press(app, 't', cmd), false)
  assert.equal(press(app, 'i', cmd), false)
  assert.equal(press(app, ']', tabKeyModifiers), false)
  assert.equal(sockets.length, 2)
  assert.equal(app.context.location.hash, '#/o/r/main/s1')

  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
})

test('worktree terminals keep the tab keys', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  for (const name of ['[', '{', ']', '}']) assert.equal(key(event(name, tabKeyModifiers)), false, `${name} stays out of the terminal`)
  for (const name of ['i', 't']) assert.equal(key(event(name, cmd)), false, `${name} stays out of the terminal`)
  assert.equal(key(event('i')), true)
  assert.equal(key(event('[', cmd)), true)
})

const barModifiers = { metaKey: true, altKey: true }

// Presses Alt+Cmd+[ when `step` is `previous`, else Alt+Cmd+], with the keys macOS gives under Alt and `modifiers`, on `target`. Returns true when the page took it.
function pressBar(app, step, target = undefined, modifiers = {}) {
  const [key, code] = step === 'previous' ? ['“', 'BracketLeft'] : ['‘', 'BracketRight']
  return press(app, key, { ...barModifiers, code, ...modifiers }, target)
}

test('Alt+Cmd+[ and Alt+Cmd+] step through the bar and wrap', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath), sessionAt('s2', worktreePath)])

  // Presses the bar key of `direction`, follows the new hash, and returns it.
  const step = async (direction) => {
    assert.ok(pressBar(app, direction), `${direction} is taken`)
    app.window.dispatch('hashchange')
    await settle()
    return app.context.location.hash
  }

  assert.equal(await step('next'), '#/o/r/main/s1')
  assert.equal(await step('next'), '#/o/r/main/s2')
  assert.equal(await step('next'), worktreeHashOf(worktreePath))
  assert.equal(await step('previous'), '#/o/r/main/s2')
  assert.deepEqual(currentBarLinks(app), ['#/o/r/main/s2'])
  assert.equal(await step('previous'), '#/o/r/main/s1')
})

test('Alt+Cmd+] with only the Terminal item stays put', async () => {
  const { app } = await worktreeApp()

  assert.ok(pressBar(app, 'next'))
  assert.ok(pressBar(app, 'previous'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(worktreePath)])
})

test('a worktree terminal passes Alt+Cmd+[ / ] to the board', async () => {
  const { app, terminals } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, code, modifiers) => ({ key: name, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  assert.equal(key(event('“', 'BracketLeft', barModifiers)), false)
  assert.equal(key(event('‘', 'BracketRight', barModifiers)), false)
  assert.equal(key(event('“', 'BracketLeft', { altKey: true })), true)

  assert.ok(pressBar(app, 'next', terminals[0].element))
  assert.equal(app.context.location.hash, '#/o/r/main/s1')
})

test('the bar keys do nothing off the worktree page or with Shift or Ctrl', async () => {
  const { app } = await worktreeApp(worktreePath, [sessionAt('s1', worktreePath)])

  for (const modifiers of [{ shiftKey: true }, { ctrlKey: true }]) assert.equal(pressBar(app, 'next', undefined, modifiers), false)
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))

  navigate(app, '#/sessions')
  assert.equal(pressBar(app, 'next'), false)
  assert.equal(pressBar(app, 'previous'), false)
  assert.equal(app.context.location.hash, '#/sessions')
})

test('Cmd+I edits the active tab name and Enter saves it', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  assert.ok(press(app, 'i', cmd, terminals[1].element))
  const input = app.elements.get('term-rename')
  assert.equal(input.value, 'shell 2')
  assert.equal(input.focused, true)
  assert.equal(input.selected, true)
  assert.deepEqual(tabNames(app), ['shell 1'])

  assert.equal(press(app, '}', tabKeyModifiers), false, 'the tab keys wait for the edit')
  assert.equal(press(app, 'i', cmd), false)

  terminals[1].blur()
  input.value = '  build  '
  assert.ok(pressInRename(app, 'Enter'))
  assert.deepEqual(tabNames(app), ['shell 1', 'build*'])
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.equal(terminals[1].focused, true)
})

test('Escape drops the edit, and a blank name keeps the old one', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = 'build'
  assert.ok(pressInRename(app, 'Escape'))
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = '   '
  pressInRename(app, 'Enter')
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('leaving the text field saves the name', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  const input = app.elements.get('term-rename')
  input.value = 'build'
  app.elements.get('term-tabs').dispatch('focusout', { target: input })
  assert.deepEqual(tabNames(app), ['build*'])
})

test('a poll keeps the text field and its draft', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  const input = app.elements.get('term-rename')
  input.value = 'bui'
  await app.run('poll()')
  assert.equal(app.elements.get('term-rename'), input)
  assert.equal(input.value, 'bui')
})

test('the edit ends when its tab closes or the page changes', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  press(app, 'i', cmd)
  sockets[1].onclose({ code: 1000, reason: '' })
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.deepEqual(tabNames(app), ['shell 1*'])

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = 'build'
  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

// Returns `worktreeApp()` on the repos page with `worktreePath` selected.
async function selectedWorktreeApp() {
  const stubs = await worktreeApp()
  const { app } = stubs
  navigate(app, '#/')

  press(app, 'ArrowUp')
  assert.equal(app.run('selectedWorktree'), worktreePath)
  return stubs
}

test('o, i and e post to the worktree open route and stay on the repos page', async () => {
  const { app, posts, sockets } = await selectedWorktreeApp()

  for (const key of ['o', 'i', 'e']) assert.ok(press(app, key), `${key} is taken`)
  await settle()
  assert.deepEqual(posts, ['github', 'intellij', 'zed'].map((tool) => `api/open/${tool}?path=${encodedWorktree}`))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(sockets.length, 0)
})

test('t opens the worktree page with one shell', async () => {
  const { app, sockets, terminals } = await selectedWorktreeApp()

  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&name=shell%201&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(terminals[0].focused, true)

  assert.ok(press(app, 'Escape'))
  app.window.dispatch('hashchange')
  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(sockets.length, 1)
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('t keeps the active shell', async () => {
  const { app } = await selectedWorktreeApp()
  press(app, 't')
  clickTab(app, 'add')
  const [, second] = tabsOf(app)
  clickTab(app, 'select', second.id)

  navigate(app, '#/')
  assert.ok(press(app, 't'))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
})

test('t with only lazygit open opens a shell', async () => {
  const { app } = await selectedWorktreeApp()

  assert.ok(press(app, 'g'))
  assert.ok(press(app, 't'))
  assert.deepEqual(tabNames(app), ['lazygit', 'shell 1*'])
  assert.deepEqual(tabsOf(app).map((tab) => tab.program), ['lazygit', 'shell'])
})

test('g opens lazygit once', async () => {
  const { app, sockets } = await selectedWorktreeApp()

  assert.ok(press(app, 'g'))
  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=lazygit&name=lazygit&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['lazygit*'])
})

test('t opens a new shell beside a failed one', async () => {
  const { app, sockets } = await selectedWorktreeApp()
  assert.ok(press(app, 't'))
  sockets[0].onclose({ code: 1006, reason: '' })

  navigate(app, '#/')
  assert.ok(press(app, 't'))
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['shell 1!', 'shell 2*'])
})

test('g opens a new lazygit beside a failed one', async () => {
  const { app, sockets } = await selectedWorktreeApp()
  assert.ok(press(app, 'g'))
  sockets[0].onclose({ code: 1006, reason: '' })

  navigate(app, '#/')
  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['lazygit!', 'lazygit*'])
})

test('the worktree keys work on a worktree page', async () => {
  const { app, posts } = await worktreeApp()

  assert.ok(press(app, 'o'))
  await settle()
  assert.deepEqual(posts, [`api/open/github?path=${encodedWorktree}`])

  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('the action keys need a selection', async () => {
  const { app, posts, sockets } = await worktreeApp()
  navigate(app, '#/')

  assert.equal(app.run('selectedWorktree'), null)
  for (const key of ['t', 'o']) assert.equal(press(app, key), false, `${key} needs a selection`)
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(sockets.length, 0)
  assert.equal(app.context.location.hash, '#/')
})

test('the action keys do nothing on an unknown worktree page', async () => {
  const { app, posts, sockets } = await worktreeApp('/s/o/r')

  for (const key of ['t', 'g', 'o']) assert.equal(press(app, key), false, `${key} needs a configured worktree`)
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(sockets.length, 0)
  assert.equal(app.context.location.hash, worktreeHashOf('/s/o/r'))

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
})

test("the worktree card's lazygit button still opens a new tab", async () => {
  const { app, sockets } = await worktreeApp()

  clickCard(app, 'lazygit')
  clickCard(app, 'lazygit')
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['lazygit', 'lazygit*'])
})

const worktreeSession = { ...session, org: 'o', repo: 'r', id: 's1', dir: 'api/file/o/r/main/s1/', worktree: worktreePath }

// Returns a dashboard on the sessions page with `shown` selected, after one poll of an api that answers `api/repos` with `reposReply()`, with fake terminal classes and the instances they create.
async function sessionsListApp(shown = worktreeSession, reposReply = undefined) {
  const stubs = await worktreeApp(worktreePath, [shown], reposReply)
  navigate(stubs.app, '#/sessions')
  stubs.app.run(`selectedDir = ${JSON.stringify(shown.dir)}`)
  return stubs
}

test('g on the list opens the worktree page with a lazygit tab', async () => {
  const { app, sockets } = await sessionsListApp()

  assert.ok(press(app, 'g'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['lazygit*'])
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=lazygit&name=lazygit&cols=100&rows=30`)
})

test('the lazygit row button opens a lazygit tab', async () => {
  const { app } = await sessionsListApp()
  assert.match(app.elements.get('page').innerHTML, /data-action="lazygit" title="Open lazygit in a tab \(g\)"/)

  const button = { dataset: { action: 'lazygit' }, closest: () => ({ dataset: { dir: worktreeSession.dir } }) }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('list').dispatch('click', { target })
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['lazygit*'])
})

test('g and the lazygit row button on a session without a listed worktree tell the user', async () => {
  for (const worktree of [undefined, '', '/s/o/r/gone', mainPath]) {
    const shown = { ...worktreeSession, worktree }
    const { app, sockets } = await sessionsListApp(shown, () => response(JSON.stringify(linkedRepos)))

    assert.ok(press(app, 'g'), `g on worktree ${JSON.stringify(worktree)}`)
    assert.equal(app.elements.get('modal').open, true)
    assert.equal(app.elements.get('modal-text').textContent, 'Terminals open only in a linked worktree.')
    closeModal(app, '')

    const button = { dataset: { action: 'lazygit' }, closest: () => ({ dataset: { dir: shown.dir } }) }
    const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
    app.elements.get('list').dispatch('click', { target })
    assert.equal(app.elements.get('modal').open, true)
    assert.equal(app.elements.get('modal-text').textContent, 'Terminals open only in a linked worktree.')

    assert.equal(app.context.location.hash, '#/sessions')
    assert.equal(sockets.length, 0)
  }
})

// Returns a dashboard on the item of `s1` of the worktree page of `worktreePath`, after one poll of an api that lists `sessions` and answers `api/repos` with `reposReply()`, with fake terminal classes, the instances they create and the URLs it posts.
async function sessionItemApp(sessions = [worktreeSession], reposReply = undefined) {
  const stubs = await worktreeApp(worktreePath, sessions, reposReply)
  navigate(stubs.app, '#/o/r/main/s1')
  await settle()
  return stubs
}

test('g on a session item selects the Terminal item with a lazygit tab', async () => {
  const { app, posts } = await sessionItemApp()

  assert.ok(press(app, 'g'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(worktreePath)])
  assert.deepEqual(tabNames(app), ['lazygit*'])
  assert.deepEqual(posts, [])
})

test("the worktree card's lazygit button on a session item selects the Terminal item with a lazygit tab", async () => {
  const { app, posts, sockets } = await sessionItemApp()

  clickCard(app, 'lazygit')
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(worktreePath)])
  assert.deepEqual(tabNames(app), ['lazygit*'])
  assert.equal(sockets.length, 1)
  assert.deepEqual(posts, [])
})

test('t on a session item selects the Terminal item with a shell tab', async () => {
  const { app, posts, sockets } = await sessionItemApp()

  assert.ok(press(app, 't'))
  await settle()
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(worktreePath)])
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&name=shell%201&cols=100&rows=30`)
  assert.deepEqual(posts, [])
})

test('g with unsaved editor changes asks to discard first', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const { sockets } = terminalStubs(app)
  const shown = app.context.location.hash
  app.context.session = { ...editSession, worktree: worktreePath }
  app.context.repos = worktreeRepos
  app.run('state.sessions = [session]')
  app.run('reposState = repos')

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.ok(press(app, 'g'))
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, shown)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.equal(sockets.length, 0)

  press(app, 'g')
  closeModal(app, 'ok')
  await settle()
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['lazygit*'])
})

test('a click or Enter on a Sessions row shows the worktree page with that session', async () => {
  const { app } = await sessionsListApp()
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.deepEqual(currentBarLinks(app), ['#/o/r/main/s1'])
  assert.ok(pageIds(app).includes('metro'))

  navigate(app, '#/sessions')
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, '#/o/r/main/s1')
  app.window.dispatch('hashchange')
  await settle()
  assert.deepEqual(currentBarLinks(app), ['#/o/r/main/s1'])
})

const otherSession = sessionAt('s2', worktreePath)

test('back from a page entered from the Sessions tab returns there', async () => {
  for (const [key, modifiers] of [['Escape'], ['l'], ['[', { metaKey: true }]]) {
    const { app } = await worktreeApp(worktreePath, [worktreeSession, otherSession])
    navigate(app, '#/sessions')
    navigate(app, '#/o/r/main/s1')
    await settle()
    navigate(app, worktreeHashOf(worktreePath))
    navigate(app, '#/o/r/main/s2')
    await settle()

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, '#/sessions')
    assert.equal(app.run('selectedDir'), otherSession.dir)
  }
})

test('back from a page entered from the Repos tab returns there', async () => {
  for (const from of ['#/', '#/terminals']) {
    const { app } = await worktreeApp(worktreePath, [worktreeSession])
    navigate(app, from)
    navigate(app, worktreeHashOf(worktreePath))
    navigate(app, '#/o/r/main/s1')
    await settle()

    assert.ok(press(app, 'Escape'))
    assert.equal(app.context.location.hash, '#/', `from ${from}`)
    assert.equal(app.run('selectedWorktree'), worktreePath)
  }
})

test('files and items of the same page keep the origin', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  navigate(app, '#/sessions')
  app.run('selectedDir = null')
  navigate(app, '#/o/r/main/s1')
  await settle()
  navigate(app, '#/o/r/main/s1/spec.md')
  await settle()
  navigate(app, worktreeHashOf(worktreePath))

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/sessions')
  assert.equal(app.run('selectedDir'), worktreeSession.dir)
})

test('a page entered again from another page takes the new origin', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  navigate(app, '#/sessions')
  navigate(app, '#/o/r/main/s1')
  await settle()
  navigate(app, '#/')
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), worktreePath)
})

const prList = [
  {
    number: 12, title: 'Fix login', author: 'alice', branch: 'alice/fix-login', draft: false, additions: 10, deletions: 3, url: 'https://github.com/akka/sdk/pull/12',
    ci: 'passing', review: 'approved', reviewers: [{ login: 'carol', state: 'approved' }, { login: 'dan', state: 'commented' }], reviewRequested: false,
  },
  {
    number: 7, title: 'Add <b>docs</b>', author: 'bob', branch: 'docs', draft: true, additions: 5, deletions: 0, url: 'https://github.com/akka/sdk/pull/7',
    ci: 'none', review: 'none', reviewers: [], reviewRequested: true,
  },
]
const fixLogin = { path: '/s/akka/sdk/fix-login', branch: 'alice/fix-login', head: null }

// Returns a reply function that answers `api/prs` with `prs` and `error`.
const prsJson = (prs, error = null) => () => response(JSON.stringify({ error, prs }))

// Returns a dashboard on `hash` after one poll of an api that answers `api/repos` with `repos`, `api/prs?path=…` with `prsReply()`
// and `api/prs/cached?path=…` with `cachedReply()`, once a reply that is ready has landed.
async function prsApp(repos, prsReply, hash = prsHashOf(sdkMain), cachedReply = () => response('', false, { status: 404 })) {
  const app = dashboard({
    fetcher: (url) => {
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply([])
      if (url.startsWith('api/prs?path=')) return prsReply()
      if (url.startsWith('api/prs/cached?path=')) return cachedReply()
      return response('', false, { status: 404 })
    },
  })
  app.context.location.hash = hash
  await app.run('poll()')
  await settle()
  return app
}

// Returns the `api/prs?path=…` URLs the dashboard fetched.
const prsCalls = (app) => app.calls.filter((url) => url.startsWith('api/prs?path='))

// Returns the `api/prs/cached?path=…` URLs the dashboard fetched.
const cachedCalls = (app) => app.calls.filter((url) => url.startsWith('api/prs/cached?path='))

// Returns the PR rows of the PRs page, each with its class, its number, its body text, its chips text and its icons HTML.
const prRows = (app) => [...app.elements.get('pr-list').innerHTML.matchAll(/<div class="(row pr[^"]*)" data-number="(\d+)">\s*<div class="body">([\s\S]*?)<\/div>\s*<div class="foot">\s*<div class="pr-chips">([\s\S]*?)<\/div>\s*<div class="actions">([\s\S]*?)<\/div>/g)]
  .map((match) => ({ class: match[1], number: Number(match[2]), text: textOf(match[3]), chips: textOf(match[4]), actions: match[5] }))

const prNumbers = (app) => prRows(app).map((row) => row.number)
const prClasses = (app) => prRows(app).map((row) => row.class)
const prListText = (app) => textOf(app.elements.get('pr-list').innerHTML)
const prCardText = (app) => textOf(app.elements.get('pr-card').innerHTML)

// Types `text` into the filter field of the PRs page.
function typePrFilter(app, text) {
  const field = app.elements.get('pr-filter')
  field.value = text
  field.dispatch('input')
}

// Clicks the row of PR `number` with `modifiers`, or outside every row when `number` is null.
function clickPr(app, number, modifiers = {}) {
  const row = { dataset: { number: String(number) } }
  const target = { closest: (selector) => (selector === '.row.pr' && number !== null ? row : null) }
  app.elements.get('pr-list').dispatch('click', { target, ...modifiers })
}

const prTexts = ['#12 Fix login alice · +10 −3 CI passing · approved', '#7 Add <b>docs</b> bob · +5 −0 no CI · no reviews']

test('the PRs page fetches the list once and shows it', async () => {
  const app = await prsApp(selectRepos, prsJson(prList))

  assert.deepEqual(prsCalls(app), ['api/prs?path=%2Fs%2Fakka%2Fsdk%2Fmain'])
  assert.deepEqual(cachedCalls(app), ['api/prs/cached?path=%2Fs%2Fakka%2Fsdk%2Fmain'])
  await app.run('poll()')
  await settle()
  assert.deepEqual(prsCalls(app), ['api/prs?path=%2Fs%2Fakka%2Fsdk%2Fmain'])
  assert.equal(cachedCalls(app).length, 1)

  assert.match(app.elements.get('page').innerHTML, /<input type="search" class="repo-filter" id="pr-filter" placeholder="Filter by number, title, author or branch"/)
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 2 PRs')
  assert.deepEqual(prRows(app).map((row) => row.text), prTexts)
  assert.doesNotMatch(app.elements.get('pr-list').innerHTML, /<b>/)

  const quoted = { error: null, repos: [{ org: 'o', repo: 'r', path: worktreePath, branch: 'main', error: null, worktrees: [] }] }
  const other = await prsApp(quoted, prsJson([]), prsHashOf(worktreePath))
  assert.deepEqual(prsCalls(other), [`api/prs?path=${encodedWorktree}`])
  assert.equal(prCardText(other), "o/r o/r/work tree's 0 PRs")
})

test('a checked-out PR shows the marker and its worktree', async () => {
  const repos = plain(selectRepos)
  repos.repos[1].worktrees.push({ path: '/s/o/r/docs', branch: 'docs', head: null })
  const app = await prsApp(repos, prsJson(prList))

  assert.deepEqual(prRows(app).map((row) => row.text), prTexts)

  repos.repos[0].worktrees.push(fixLogin)
  await app.run('poll()')
  await settle()
  assert.deepEqual(prRows(app).map((row) => row.text), ['#12 Fix login alice · +10 −3 ● akka/sdk/fix-login CI passing · approved', prTexts[1]])
  assert.equal(prsCalls(app).length, 1)
  assert.equal(app.run('prWorktree(reposState.repos[0], prsState.prs[0]).path'), fixLogin.path)
  assert.equal(app.run('prWorktree(reposState.repos[0], prsState.prs[1])'), undefined)
})

test('a PR row shows its chips, its CI and review states, and its reviewers on hover', async () => {
  const failing = { ...prList[0], number: 3, ci: 'failing', review: 'changes-requested', reviewers: [{ login: 'erin', state: 'changes requested' }], draft: true }
  const idle = { ...prList[1], number: 2, draft: false, reviewRequested: false, ci: 'pending' }
  const app = await prsApp(selectRepos, prsJson([...prList, failing, idle]))
  const html = app.elements.get('pr-list').innerHTML

  assert.deepEqual(prRows(app).map((row) => [row.number, row.chips]), [[2, ''], [12, 'reviewed'], [7, 'to review draft'], [3, 'reviewed draft']])
  assert.match(html, /<span class="pr-chip reviewed">reviewed<\/span>/)
  assert.match(html, /<span class="pr-chip to-review">to review<\/span><span class="pr-chip draft">draft<\/span>/)

  assert.match(html, /<span class="ci passing">CI passing<\/span> · <span class="review approved" title="Reviewed by carol \(approved\), dan \(commented\)">approved<\/span>/)
  assert.match(html, /<span class="ci none">no CI<\/span> · <span class="review none" title="No reviews yet">no reviews<\/span>/)
  assert.match(html, /<span class="ci failing">CI failing<\/span> · <span class="review changes-requested" title="Reviewed by erin \(changes requested\)">changes requested<\/span>/)
  assert.match(html, /<span class="ci pending">CI running<\/span>/)
})

test('the PR list shows checked-out PRs, then open PRs without reviews, then open PRs with reviews, then drafts', async () => {
  const pr = (number, fields) => ({ ...prList[1], number, branch: `pr-${number}`, draft: false, reviewers: [], reviewRequested: false, ...fields })
  const reviewed = [{ login: 'carol', state: 'approved' }]
  const prs = [
    pr(1, { draft: true }),
    pr(2, { reviewers: reviewed }),
    pr(3, {}),
    pr(4, { draft: true, branch: 'alice/fix-login' }),
    pr(5, { reviewRequested: true }),
    pr(6, { reviewers: reviewed, branch: 'docs' }),
    pr(7, { reviewers: reviewed }),
    pr(8, { draft: true, reviewers: reviewed }),
  ]
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin, { path: '/s/akka/sdk/docs', branch: 'docs', head: null })
  const app = await prsApp(repos, prsJson(prs))

  assert.deepEqual(prNumbers(app), [4, 6, 3, 5, 2, 7, 1, 8])

  assert.ok(press(app, 'ArrowDown'))
  assert.ok(press(app, 'ArrowDown'))
  assert.equal(app.run('selectedPr'), 6)
})

// Returns the text of the worktree card.
const worktreeCardText = (app) => textOf(app.elements.get('worktree-card').innerHTML)

// The repos of `selectRepos` with worktrees on the branches of both PRs of `prList`.
const prRepos = () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin, { path: '/s/akka/sdk/docs', branch: 'docs', head: null })
  return repos
}

test('the worktree card is the row of the open PR of its branch as the PRs page shows it', async () => {
  const app = await prsApp(prRepos(), prsJson(prList), worktreeHashOf(fixLogin.path))
  const card = app.elements.get('worktree-card').innerHTML

  assert.equal(worktreeCardText(app), '#12 Fix login alice · +10 −3 ● akka/sdk/fix-login CI passing · approved reviewed')
  assert.match(card, /^\s*<div class="row pr">\s*<div class="body">/)
  assert.doesNotMatch(card, /session-card/)
  assert.deepEqual([...card.matchAll(/data-action="([^"]+)"/g)].map((match) => match[1]), ['lazygit', 'iterm', 'zed', 'intellij', 'github'])
  assert.match(card, /<span class="ci passing">CI passing<\/span> · <span class="review approved" title="Reviewed by carol \(approved\), dan \(commented\)">approved<\/span>/)
  assert.doesNotMatch(card, /data-number=/)
  assert.deepEqual(prsCalls(app), ['api/prs?path=%2Fs%2Fakka%2Fsdk%2Fmain'])
  assert.deepEqual(cachedCalls(app), ['api/prs/cached?path=%2Fs%2Fakka%2Fsdk%2Fmain'])

  await app.run('poll()')
  await settle()
  assert.equal(prsCalls(app).length, 1)

  navigate(app, worktreeHashOf('/s/akka/sdk/docs'))
  await settle()
  assert.equal(worktreeCardText(app), '#7 Add <b>docs</b> bob · +5 −0 ● akka/sdk/docs no CI · no reviews draft')
  assert.doesNotMatch(app.elements.get('worktree-card').innerHTML, /<b>/)
  assert.equal(prsCalls(app).length, 1)

  navigate(app, '#/')
  navigate(app, worktreeHashOf(fixLogin.path))
  await settle()
  assert.equal(prsCalls(app).length, 2)
})

test('the worktree card shows no PR without an open PR on its branch', async () => {
  const app = await prsApp(prRepos(), prsJson(prList), worktreeHashOf(pathA))
  assert.equal(worktreeCardText(app), 'akka/sdk akka/sdk/alpha alpha')
  assert.doesNotMatch(app.elements.get('worktree-card').innerHTML, /row pr/)

  const failed = await prsApp(prRepos(), prsJson([], 'not logged in'), worktreeHashOf(fixLogin.path))
  assert.equal(worktreeCardText(failed), 'akka/sdk akka/sdk/fix-login alice/fix-login')

  const main = await prsApp(prRepos(), prsJson(prList), worktreeHashOf(sdkMain))
  assert.deepEqual(prsCalls(main), [])
  assert.deepEqual(cachedCalls(main), [])
})

test('the worktree card shows the cached PR until the fresh list lands', async () => {
  let release
  const pending = new Promise((resolve) => { release = resolve })
  const stale = [{ ...prList[0], ci: 'pending' }]
  const app = await prsApp(prRepos(), () => pending, worktreeHashOf(fixLogin.path), prsJson(stale))

  assert.match(worktreeCardText(app), /CI running · approved/)
  release(response(JSON.stringify({ error: null, prs: prList })))
  await settle()
  assert.match(worktreeCardText(app), /CI passing · approved/)

  const failed = await prsApp(prRepos(), prsJson([], 'offline'), worktreeHashOf(fixLogin.path), prsJson(stale))
  assert.match(worktreeCardText(failed), /CI running · approved/)
})

test('the row icons show their keys', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)
  const app = await prsApp(repos, prsJson(prList))

  // Returns the action, title and PR number of each icon of `row`.
  const icons = (row) => [...row.actions.matchAll(/data-action="([^"]*)" title="([^"]*)" data-number="(\d+)"/g)]
    .map((match) => `${match[1]}: ${match[2]} #${match[3]}`)

  const [checkedOut, fresh] = prRows(app)
  assert.deepEqual(icons(checkedOut), [
    'worktree: Open worktree page (c) #12',
    'zed: Open in Zed (e) #12',
    'intellij: Open in IntelliJ (i) #12',
    'github: Open on GitHub (o) #12',
    'refresh: Refresh PR (u) #12',
  ])
  assert.deepEqual(icons(fresh), [
    'worktree: Check out and open worktree page (c) #7',
    'zed: Open in Zed (e) #7',
    'intellij: Open in IntelliJ (i) #7',
    'github: Open on GitHub (o) #7',
    'refresh: Refresh PR (u) #7',
  ])

  const panel = '<rect x="2" y="2.5" width="12" height="11" rx="1.5"/><path d="M6.5 2.5v11"/>'
  assert.equal(app.run('ACTION_ICONS.worktree'), panel)
  assert.ok(checkedOut.actions.includes(panel))
})

test('the PRs page states', async () => {
  let release
  const pending = new Promise((resolve) => { release = resolve })
  const app = await prsApp(selectRepos, () => pending)

  assert.equal(prListText(app), 'loading…')
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main')
  release(response(JSON.stringify({ error: null, prs: [] })))
  await settle()
  assert.equal(prListText(app), 'No open PRs.')
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 0 PRs')

  const failed = await prsApp(selectRepos, () => response(JSON.stringify({ error: 'not logged in' })))
  assert.equal(prListText(failed), 'not logged in')
  assert.equal(prCardText(failed), 'akka/sdk akka/sdk/main')

  const broken = await prsApp(selectRepos, () => response('', false, { status: 500 }))
  assert.equal(prListText(broken), '500 api/prs?path=%2Fs%2Fakka%2Fsdk%2Fmain')

  const offline = await prsApp(selectRepos, () => { throw new Error('disconnected') })
  assert.equal(prListText(offline), 'disconnected')

  const unknown = await prsApp(selectRepos, prsJson(prList), prsHashOf('/s/akka/sdk'))
  assert.equal(pageText(unknown), 'not a configured repo')
  assert.deepEqual(prsCalls(unknown), [])
})

test('the PRs page shows the cached list while the fresh list loads', async () => {
  const releases = []
  const fresh = () => new Promise((resolve) => releases.push(resolve))
  const app = await prsApp(selectRepos, fresh, prsHashOf(sdkMain), prsJson(prList))

  assert.deepEqual(prNumbers(app), [12, 7])
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 2 PRs refreshing…')

  press(app, 'ArrowDown')
  releases[0](prsJson([prList[1]])())
  await settle()
  assert.deepEqual(prNumbers(app), [7])
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 1 PR')
  assert.equal(app.run('selectedPr'), null, 'a PR the fresh list drops loses the selection')

  const empty = await prsApp(selectRepos, fresh, prsHashOf(sdkMain), prsJson(null))
  assert.equal(prListText(empty), 'loading…')
  assert.equal(prCardText(empty), 'akka/sdk akka/sdk/main')
  releases[1](prsJson(prList)())
  await settle()
  assert.deepEqual(prNumbers(empty), [12, 7])
})

test('a cached list that lands after the fresh list is dropped', async () => {
  let release
  const late = () => new Promise((resolve) => { release = resolve })
  const app = await prsApp(selectRepos, prsJson([prList[1]]), prsHashOf(sdkMain), late)

  assert.deepEqual(prNumbers(app), [7])
  release(prsJson(prList)())
  await settle()
  assert.deepEqual(prNumbers(app), [7])
})

test('a failed refresh keeps the cached list below its error', async () => {
  const app = await prsApp(selectRepos, prsJson([], 'not logged in'), prsHashOf(sdkMain), prsJson(prList))

  assert.match(prListText(app), /^not logged in/)
  assert.deepEqual(prNumbers(app), [12, 7])
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 2 PRs')

  typePrFilter(app, 'zzz')
  assert.equal(prListText(app), 'not logged in No PR matches.')
})

test('leaving and reopening the PRs page fetches again', async () => {
  const app = await prsApp(selectRepos, prsJson(prList))
  press(app, 'ArrowDown')
  typePrFilter(app, 'fix')

  navigate(app, '#/')
  assert.equal(app.run('prsState'), null)
  assert.equal(app.run('selectedPr'), null)
  assert.equal(app.run('prFilter'), '')

  navigate(app, prsHashOf(sdkMain))
  await settle()
  assert.equal(prsCalls(app).length, 2)
  assert.equal(app.elements.get('pr-filter').value, '')
  assert.deepEqual(prClasses(app), ['row pr', 'row pr'])

  const releases = []
  const late = await prsApp(selectRepos, () => new Promise((resolve) => releases.push(resolve)))
  navigate(late, '#/')
  navigate(late, prsHashOf(sdkMain))
  releases[1](prsJson([prList[1]])())
  await settle()
  releases[0](prsJson(prList)())
  await settle()
  assert.deepEqual(prNumbers(late), [7], 'a reply for a page left meanwhile is dropped')

  const another = await prsApp(selectRepos, () => new Promise((resolve) => releases.push(resolve)))
  navigate(another, '#/')
  releases[2](prsJson(prList)())
  await settle()
  assert.equal(another.run('prsState'), null)
  assert.match(another.elements.get('page').innerHTML, /id="repo-list"/)
})

test('arrow keys select PRs and stop at the ends', async () => {
  const app = await prsApp(selectRepos, prsJson(prList))
  const steps = [
    ['ArrowDown', 12, ['row pr selected', 'row pr']],
    ['ArrowDown', 7, ['row pr', 'row pr selected']],
    ['ArrowDown', 7, ['row pr', 'row pr selected']],
    ['ArrowUp', 12, ['row pr selected', 'row pr']],
    ['ArrowUp', 12, ['row pr selected', 'row pr']],
  ]

  for (const [key, number, classes] of steps) {
    assert.ok(press(app, key), `${key} is taken`)
    assert.equal(app.run('selectedPr'), number)
    assert.deepEqual(prClasses(app), classes)
  }

  await app.run('poll()')
  assert.equal(app.run('selectedPr'), 12)

  const fresh = await prsApp(selectRepos, prsJson(prList))
  assert.ok(press(fresh, 'ArrowUp'))
  assert.equal(fresh.run('selectedPr'), 7)
})

test('slash focuses the PR filter and the filter narrows the rows', async () => {
  const bump = {
    number: 3, title: 'Bump deps', author: 'alice', branch: 'deps', draft: false, additions: 1, deletions: 1, url: 'https://github.com/akka/sdk/pull/3',
    ci: 'passing', review: 'commented', reviewers: [{ login: 'carol', state: 'commented' }], reviewRequested: false,
  }
  const app = await prsApp(selectRepos, prsJson([...prList, bump]))
  const field = app.elements.get('pr-filter')
  const hash = prsHashOf(sdkMain)

  assert.ok(press(app, '/'))
  assert.equal(field.focused, true)

  typePrFilter(app, 'alice fix')
  assert.deepEqual(prNumbers(app), [12])
  typePrFilter(app, 'ALICE')
  assert.deepEqual(prNumbers(app), [12, 3])
  typePrFilter(app, '#7')
  assert.deepEqual(prNumbers(app), [7])
  typePrFilter(app, 'deps')
  assert.deepEqual(prNumbers(app), [3])
  typePrFilter(app, 'zzz')
  assert.equal(prListText(app), 'No PR matches.')
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 3 PRs')

  typePrFilter(app, 'alice')
  assert.ok(press(app, 'ArrowDown', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.run('selectedPr'), 12)

  field.focus()
  assert.ok(press(app, 'ArrowDown', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.run('selectedPr'), 3)

  field.focus()
  for (const key of ['l', 'c']) assert.equal(press(app, key, {}, field), false, `${key} is text`)
  assert.equal(field.focused, true)
  assert.ok(press(app, 'Escape', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.context.location.hash, hash)

  await app.run('poll()')
  await settle()
  assert.equal(app.elements.get('pr-filter'), field)
  assert.equal(field.value, 'alice')
  assert.deepEqual(prNumbers(app), [12, 3])
  assert.equal(prsCalls(app).length, 1)

  typePrFilter(app, 'fix')
  assert.equal(app.run('selectedPr'), null, 'a PR the filter hides loses the selection')

  field.focus()
  assert.ok(press(app, 'Enter', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.run('selectedPr'), 12)
  assert.equal(app.context.location.hash, hash)
})

test('a click on a PR row selects it', async () => {
  const app = await prsApp(selectRepos, prsJson(prList))

  clickPr(app, 7)
  assert.equal(app.run('selectedPr'), 7)
  assert.deepEqual(prClasses(app), ['row pr', 'row pr selected'])

  clickPr(app, null)
  assert.equal(app.run('selectedPr'), 7)
})

test('back keys return to the repos page with the main row selected', async () => {
  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const app = await prsApp(selectRepos, prsJson(prList))

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, '#/')
    assert.equal(app.run('selectedWorktree'), sdkMain)

    app.window.dispatch('hashchange')
    assert.deepEqual(selectedRows(app), [prsHashOf(sdkMain)])
  }
})

const encodedFixLogin = '%2Fs%2Fakka%2Fsdk%2Ffix-login'

// Returns the checkout URL of PR `number` of `sdkMain`.
const checkoutUrl = (number) => `api/prs/checkout?path=%2Fs%2Fakka%2Fsdk%2Fmain&number=${number}`

// Returns a dashboard on the PRs page of `sdkMain` after one poll, with fake terminal classes, the POST URLs, and the hash at each `api/repos` fetch.
// It answers `api/repos` with `repos`, `api/sessions` with `sessions`, `POST api/prs/open` with `openReply()`,
// and each checkout and PR view with the reply a test passes to the next of `checkouts` and `views`.
async function checkoutApp(repos = plain(selectRepos), openReply = () => response('', true, { status: 204 }), sessions = []) {
  const posts = []
  const checkouts = []
  const views = []
  const reposHashes = []
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') {
        posts.push(url)
        if (url.startsWith('api/prs/checkout?')) return new Promise((resolve) => checkouts.push(resolve))
        if (url.startsWith('api/prs/open?')) return openReply()
        return response('', true, { status: 204 })
      }
      if (url === 'api/repos') {
        reposHashes.push(app.context.location.hash)
        return response(JSON.stringify(repos))
      }
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      if (url.startsWith('api/prs?path=')) return prsJson(prList)()
      if (url.startsWith('api/prs/view?')) return new Promise((resolve) => views.push(resolve))
      return response('', false, { status: 404 })
    },
  })
  const stubs = terminalStubs(app)
  app.context.location.hash = prsHashOf(sdkMain)
  await app.run('poll()')
  await settle()
  return { app, posts, checkouts, views, repos, reposHashes, ...stubs }
}

// Answers the first pending checkout with the path of `worktree`, which `api/repos` lists from then on, and waits for the board.
async function finishCheckout({ checkouts, repos }, worktree = fixLogin) {
  repos.repos[0].worktrees.push(worktree)
  checkouts.shift()(response(JSON.stringify({ path: worktree.path })))
  await settle()
}

// Clicks the icon of `action` on the row of PR `number`.
function clickPrIcon(app, number, action) {
  const button = { dataset: { action, number: String(number) } }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('pr-list').dispatch('click', { target })
}

test('c on a checked-out PR goes to its worktree page', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)

  for (const key of ['c', 'Enter']) {
    const { app, posts } = await checkoutApp(repos)
    press(app, 'ArrowDown')

    assert.ok(press(app, key), `${key} is taken`)
    await settle()
    assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
    assert.deepEqual(posts, [])
  }
})

test('c on a new PR checks it out, reloads the repos, then goes to the worktree page', async () => {
  const stubs = await checkoutApp()
  const { app, posts, reposHashes } = stubs
  const hash = prsHashOf(sdkMain)
  press(app, 'ArrowDown')

  assert.ok(press(app, 'c'))
  await settle()
  assert.deepEqual(posts, [checkoutUrl(12)])
  assert.deepEqual(prClasses(app), ['row pr selected busy', 'row pr'])
  assert.match(prRows(app)[0].text, /checking out…/)
  assert.equal(app.context.location.hash, hash)
  assert.deepEqual(reposHashes, [hash])

  await finishCheckout(stubs)
  assert.deepEqual(reposHashes, [hash, hash])
  assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
  assert.equal(app.run('prBusy.size'), 0)
  assert.deepEqual(prClasses(app), ['row pr selected', 'row pr'])
  assert.match(prRows(app)[0].text, /● akka\/sdk\/fix-login/)
})

test('a second key during a checkout sends nothing', async () => {
  const stubs = await checkoutApp()
  const { app, posts } = stubs
  press(app, 'ArrowDown')

  assert.ok(press(app, 'c'))
  press(app, 'c')
  press(app, 'e')
  clickPrIcon(app, 12, 'zed')
  await settle()
  assert.deepEqual(posts, [checkoutUrl(12)])

  await finishCheckout(stubs)
  assert.deepEqual(posts, [checkoutUrl(12)])
  assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
})

test('r opens a claude tab on the worktree page', async () => {
  const stubs = await checkoutApp()
  const { app, posts, sockets } = stubs
  press(app, 'ArrowDown')

  assert.ok(press(app, 'r'))
  await finishCheckout(stubs)
  assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedFixLogin}&program=claude&name=claude&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['claude*'])

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['claude', 'shell 1*'])

  navigate(app, prsHashOf(sdkMain))
  await settle()
  press(app, 'ArrowDown')
  assert.ok(press(app, 'r'))
  await settle()
  assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['claude*', 'shell 1'])
  assert.deepEqual(posts, [checkoutUrl(12)])
})

test('e and i check out, then open the editor and stay on the PRs page', async () => {
  for (const [key, tool] of [['e', 'zed'], ['i', 'intellij']]) {
    const stubs = await checkoutApp()
    const { app, posts, sockets } = stubs
    const open = `api/open/${tool}?path=${encodedFixLogin}`
    press(app, 'ArrowDown')

    assert.ok(press(app, key), `${key} is taken`)
    await finishCheckout(stubs)
    assert.deepEqual(posts, [checkoutUrl(12), open])
    assert.equal(app.context.location.hash, prsHashOf(sdkMain))
    assert.equal(sockets.length, 0)

    assert.ok(press(app, key))
    await settle()
    assert.deepEqual(posts, [checkoutUrl(12), open, open])
  }
})

test('o posts to the PR open route', async () => {
  const { app, posts, checkouts } = await checkoutApp()
  press(app, 'ArrowDown')

  assert.ok(press(app, 'o'))
  await settle()
  assert.deepEqual(posts, ['api/prs/open?path=%2Fs%2Fakka%2Fsdk%2Fmain&number=12'])
  assert.equal(checkouts.length, 0)
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  assert.notEqual(app.elements.get('modal').open, true)

  const failing = await checkoutApp(plain(selectRepos), () => response('not logged in\n', false))
  press(failing.app, 'ArrowDown')
  assert.ok(press(failing.app, 'o'))
  await settle()
  assert.equal(failing.app.elements.get('modal').open, true)
  assert.equal(failing.app.elements.get('modal-text').textContent, 'Could not open PR #12 in GitHub: not logged in')
})

test('the row icons run the same actions', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)
  const stubs = await checkoutApp(repos)
  const { app, posts } = stubs

  clickPrIcon(app, 12, 'worktree')
  await settle()
  assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
  assert.deepEqual(posts, [])

  navigate(app, prsHashOf(sdkMain))
  await settle()
  for (const action of ['zed', 'intellij', 'github']) {
    clickPrIcon(app, 12, action)
    await settle()
  }
  assert.deepEqual(posts, [
    `api/open/zed?path=${encodedFixLogin}`,
    `api/open/intellij?path=${encodedFixLogin}`,
    'api/prs/open?path=%2Fs%2Fakka%2Fsdk%2Fmain&number=12',
  ])
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  assert.equal(app.run('selectedPr'), 12, 'the worktree icon selects its PR')

  const docs = { path: '/s/akka/sdk/docs', branch: 'docs', head: null }
  clickPrIcon(app, 7, 'worktree')
  await settle()
  assert.deepEqual(posts.slice(3), [checkoutUrl(7)])
  assert.deepEqual(prClasses(app), ['row pr selected', 'row pr busy'])

  await finishCheckout(stubs, docs)
  assert.equal(app.context.location.hash, worktreeHashOf(docs.path))
  assert.equal(app.run('selectedPr'), 7)
})

test('Cmd+click on a checked-out PR opens its worktree page in a new app window', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)
  const { app, posts } = await checkoutApp(repos)
  const opened = appOpener(app)

  clickPr(app, 12, { metaKey: true })
  await settle()
  assert.deepEqual(opened, [worktreeHashOf(fixLogin.path)])
  assert.deepEqual(posts, [])
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  assert.equal(app.run('selectedPr'), 12)
})

test('Cmd+click on a new PR checks it out, then opens its worktree page in a new app window', async () => {
  const stubs = await checkoutApp()
  const { app, posts } = stubs
  const opened = appOpener(app)

  clickPr(app, 12, { metaKey: true })
  await settle()
  assert.deepEqual(posts, [checkoutUrl(12)])
  assert.deepEqual(prClasses(app), ['row pr selected busy', 'row pr'])
  assert.deepEqual(opened, [])

  clickPr(app, 12, { metaKey: true })
  await settle()
  assert.deepEqual(posts, [checkoutUrl(12)])

  await finishCheckout(stubs)
  assert.deepEqual(opened, [worktreeHashOf(fixLogin.path)])
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
})

test('Cmd+click on a PR in a browser only selects it', async () => {
  const { app, posts } = await checkoutApp()

  clickPr(app, 12, { metaKey: true })
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(app.run('selectedPr'), 12)
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
})

test('a failed checkout tells the error and clears the busy state', async () => {
  const { app, posts, checkouts, reposHashes } = await checkoutApp()
  const hash = prsHashOf(sdkMain)
  const modal = app.elements.get('modal')
  press(app, 'ArrowDown')

  assert.ok(press(app, 'c'))
  checkouts.shift()(response('Branch x exists locally without a worktree', false, { status: 409 }))
  await settle()
  assert.equal(modal.open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not check out PR #12: Branch x exists locally without a worktree')
  assert.deepEqual(prClasses(app), ['row pr selected', 'row pr'])
  assert.equal(app.run('prBusy.size'), 0)
  assert.equal(app.context.location.hash, hash)
  assert.deepEqual(reposHashes, [hash])

  closeModal(app, '')
  assert.ok(press(app, 'c'))
  checkouts.shift()({ ...response('', false, { status: 500 }), statusText: 'Internal Server Error' })
  await settle()
  assert.equal(app.elements.get('modal-text').textContent, 'Could not check out PR #12: 500 Internal Server Error')
  assert.deepEqual(posts, [checkoutUrl(12), checkoutUrl(12)])
  assert.equal(app.context.location.hash, hash)
})

test('leaving the PRs page during a checkout stays where the user went', async () => {
  for (const key of ['c', 'r']) {
    const stubs = await checkoutApp()
    const { app, sockets, reposHashes } = stubs
    press(app, 'ArrowDown')

    assert.ok(press(app, key), `${key} is taken`)
    navigate(app, '#/sessions')
    await finishCheckout(stubs)
    assert.deepEqual(reposHashes, [prsHashOf(sdkMain), '#/sessions'])
    assert.equal(app.context.location.hash, '#/sessions')
    assert.equal(sockets.length, 0)
  }

  const stubs = await checkoutApp()
  const { app, posts } = stubs
  press(app, 'ArrowDown')

  assert.ok(press(app, 'e'))
  navigate(app, '#/sessions')
  await finishCheckout(stubs)
  assert.deepEqual(posts, [checkoutUrl(12), `api/open/zed?path=${encodedFixLogin}`])
  assert.equal(app.context.location.hash, '#/sessions')
})

// Returns the view URL of PR `number` of `sdkMain`.
const viewUrl = (number) => `api/prs/view?path=%2Fs%2Fakka%2Fsdk%2Fmain&number=${number}`

// Returns the `api/prs/view` URLs the dashboard fetched.
const viewCalls = (app) => app.calls.filter((url) => url.startsWith('api/prs/view?'))

test('u refreshes the selected PR and replaces its row', async () => {
  const { app, views } = await checkoutApp()
  press(app, 'ArrowDown')

  assert.ok(press(app, 'u'))
  press(app, 'u')
  assert.deepEqual(viewCalls(app), [viewUrl(12)], 'a second key during a refresh sends nothing')
  assert.deepEqual(prClasses(app), ['row pr selected busy', 'row pr'])
  assert.match(prRows(app)[0].text, /refreshing…/)

  views.shift()(response(JSON.stringify({ error: null, pr: { ...prList[0], title: 'Fix login v2', ci: 'failing' } })))
  await settle()
  assert.deepEqual(prClasses(app), ['row pr selected', 'row pr'])
  assert.equal(prRows(app)[0].text, '#12 Fix login v2 alice · +10 −3 CI failing · approved')
  assert.equal(app.run('prRefreshing.size'), 0)
  assert.equal(prsCalls(app).length, 1)
})

test('the refresh icon drops a PR that is no longer open', async () => {
  const { app, views } = await checkoutApp()

  clickPrIcon(app, 7, 'refresh')
  assert.deepEqual(viewCalls(app), [viewUrl(7)])
  views.shift()(response(JSON.stringify({ error: null, pr: null })))
  await settle()
  assert.deepEqual(prNumbers(app), [12])
  assert.equal(prCardText(app), 'akka/sdk akka/sdk/main 1 PR')
})

test('a failed refresh tells the error and keeps the row', async () => {
  const { app, views } = await checkoutApp()
  press(app, 'ArrowDown')

  assert.ok(press(app, 'u'))
  views.shift()(response(JSON.stringify({ error: 'not logged in', pr: null })))
  await settle()
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not refresh PR #12: not logged in')
  assert.deepEqual(prNumbers(app), [12, 7])
  assert.deepEqual(prClasses(app), ['row pr selected', 'row pr'])

  closeModal(app, '')
  assert.ok(press(app, 'u'))
  views.shift()(response('', false, { status: 500 }))
  await settle()
  assert.equal(app.elements.get('modal-text').textContent, `Could not refresh PR #12: 500 ${viewUrl(12)}`)
  assert.deepEqual(prNumbers(app), [12, 7])
})

test('back from a page entered from a PRs page restores its selection and filter', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)
  const fixSession = { ...session, org: 'akka', repo: 'sdk', branch: 'fix-login', id: 's2', dir: 'api/file/akka/sdk/fix-login/s2/', worktree: fixLogin.path }

  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const { app } = await checkoutApp(repos, undefined, [fixSession])
    typePrFilter(app, 'fix')
    press(app, 'ArrowDown')

    assert.ok(press(app, 'c'))
    await settle()
    app.window.dispatch('hashchange')
    assert.equal(app.context.location.hash, worktreeHashOf(fixLogin.path))
    assert.equal(app.run('prFilter'), '')

    navigate(app, '#/akka/sdk/fix-login/s2')
    await settle()
    navigate(app, worktreeHashOf(fixLogin.path))

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, prsHashOf(sdkMain))
    app.window.dispatch('hashchange')
    await settle()
    assert.equal(app.elements.get('pr-filter').value, 'fix')
    assert.deepEqual(prClasses(app), ['row pr selected'])
    assert.equal(app.run('selectedPr'), 12)
  }
})

test('a worktree page opened another way returns to the repos page', async () => {
  const repos = plain(selectRepos)
  repos.repos[0].worktrees.push(fixLogin)
  const { app } = await checkoutApp(repos)
  press(app, 'ArrowDown')

  assert.ok(press(app, 'c'))
  await settle()
  app.window.dispatch('hashchange')

  navigate(app, '#/')
  navigate(app, worktreeHashOf(fixLogin.path))
  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), fixLogin.path)
})

// Returns a standalone dashboard opened on `hash`, before any poll of an api that lists `sessions`, answers `api/repos` with `repos` and accepts every POST.
function standaloneDashboard(hash, sessions = [], repos = worktreeRepos) {
  return dashboard({
    search: '?standalone',
    hash,
    fetcher: (url, options) => {
      if (options?.method === 'POST') return response('', true, { status: 204 })
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
}

// Returns the dashboard of `standaloneDashboard(hash, sessions, repos)` after one poll.
async function standaloneApp(hash, sessions = [], repos = worktreeRepos) {
  const app = standaloneDashboard(hash, sessions, repos)
  await app.run('poll()')
  await settle()
  return app
}

// Returns true when `app` may show the page of `hash`.
const scoped = (app, hash) => app.run(`inScope(parseRoute(${JSON.stringify(hash)}))`)

test('the main window keeps the header and every route', () => {
  const app = dashboard()

  assert.ok(!app.elements.get('board-header').hidden)
  assert.ok(!app.elements.get('board-footer').hidden)
  assert.equal(app.run('standaloneRoot'), null)
  assert.equal(scoped(app, '#/sessions'), true)
  assert.equal(app.run('backTarget("#/")'), '#/')
})

test('a standalone window hides the header and keeps the footer', () => {
  const app = standaloneDashboard(worktreeHashOf(worktreePath))

  assert.equal(app.elements.get('board-header').hidden, true)
  assert.ok(!app.elements.get('board-footer').hidden)
  assert.deepEqual(plain(app.run('standaloneRoot')), { kind: 'worktree', path: worktreePath })
})

test('the footer shows the server version with a v prefix', async () => {
  const app = dashboard({ fetcher: (url) => (url === 'api/version' ? response('{"version":"0.1.4-3e24a0f1"}') : response('', false)) })

  await app.run('loadVersion()')

  assert.ok(app.calls.includes('api/version'))
  assert.equal(app.elements.get('version').textContent, 'v0.1.4-3e24a0f1')
})

test('the footer stays empty when the version call fails or holds no version', async () => {
  const fetchers = [
    () => response('', false),
    () => { throw new Error('disconnected') },
    () => response('not json'),
    () => response('{}'),
    () => response('{"version":3}'),
    () => response('{"version":""}'),
  ]
  for (const fetcher of fetchers) {
    const app = dashboard({ fetcher })
    await app.run('loadVersion()')
    assert.equal(app.elements.get('version').textContent, '')
  }
})

// Returns the actions of the icons on the worktree card.
const cardActions = (app) => [...app.elements.get('worktree-card').innerHTML.matchAll(/data-action="([^"]*)"/g)].map((match) => match[1])

test('Cmd+0 in a standalone app window opens the main board without navigating away', async () => {
  for (const hash of [worktreeHashOf(worktreePath), '#/o/r/main/s1', prsHashOf(sdkMain)]) {
    const commands = []
    const app = dashboard({
      search: '?standalone',
      hash,
      tauri: { core: { invoke: async (command) => { commands.push(command) } } },
    })

    assert.equal(press(app, '0', cmd), true, hash)
    await settle()
    assert.deepEqual(commands, ['open_main_window'])
    assert.equal(app.context.location.hash, hash)
  }
})

test('Cmd+0 does nothing in the main app window or a browser page', () => {
  for (const options of [{ tauri: {} }, {}, { search: '?standalone', hash: '#/o/r/main/s1' }]) {
    const app = dashboard(options)
    assert.equal(press(app, '0', cmd), false)
  }
})

test('a failure to open the main board shows an error and keeps the standalone page', async () => {
  const hash = '#/o/r/main/s1'
  const app = dashboard({
    search: '?standalone',
    hash,
    tauri: { core: { invoke: async () => { throw new Error('denied') } } },
  })

  press(app, '0', cmd)
  await settle()
  assert.equal(app.elements.get('modal-text').textContent, 'Could not open Jam Board: Error: denied')
  assert.equal(app.context.location.hash, hash)
})

test('the new-window icon opens a main-window worktree page standalone before returning to Repos', async () => {
  const { app } = await worktreeApp()
  const hash = app.context.location.hash
  const opened = []
  let finish
  app.window.__TAURI__ = {
    core: {
      invoke: (command, args) => {
        assert.equal(command, 'open_window')
        opened.push(args.hash)
        return new Promise((resolve) => { finish = resolve })
      },
    },
  }
  app.run('render()')

  assert.deepEqual(cardActions(app), ['lazygit', 'iterm', 'zed', 'intellij', 'github', 'detach'])
  assert.match(app.elements.get('worktree-card').innerHTML, /data-action="detach" title="Open in a new window \(⌘N\)"/)
  clickCard(app, 'detach')
  assert.deepEqual(opened, [hash])
  assert.equal(app.context.location.hash, hash)

  finish()
  await settle()
  assert.equal(app.context.location.hash, '#/')
  app.window.dispatch('hashchange')
  assert.match(app.elements.get('page').innerHTML, /id="repo-list"/)
})

test('Cmd+N opens a main-window worktree page in its own window', async () => {
  const { app } = await worktreeApp()
  const hash = app.context.location.hash
  const opened = []
  app.window.__TAURI__ = { core: { invoke: async (command, args) => { opened.push(`${command} ${args.hash}`) } } }

  assert.equal(press(app, 'n', cmd), true)
  await settle()
  assert.deepEqual(opened, [`open_window ${hash}`])
  assert.equal(app.context.location.hash, '#/')
})

test('the new-window icon and Cmd+N apply only to worktree pages of the main app window', async () => {
  const { app } = await worktreeApp()
  assert.ok(!cardActions(app).includes('detach'), 'no icon in a browser')
  assert.equal(press(app, 'n', cmd), false, 'no shortcut in a browser')

  app.window.__TAURI__ = { core: { invoke: async () => {} } }
  for (const hash of ['#/', '#/sessions', '#/terminals', prsHashOf(mainPath)]) {
    navigate(app, hash)
    assert.equal(press(app, 'n', cmd), false, hash)
  }

  const standalone = await standaloneApp(worktreeHashOf(worktreePath))
  standalone.window.__TAURI__ = { core: { invoke: async () => {} } }
  standalone.run('render()')
  assert.ok(!cardActions(standalone).includes('detach'))
  assert.equal(press(standalone, 'n', cmd), false)
})

test('a failed detach keeps the main window on the worktree and reports the error', async () => {
  const { app } = await worktreeApp()
  const hash = app.context.location.hash
  app.window.__TAURI__ = { core: { invoke: async () => { throw new Error('denied') } } }

  clickCard(app, 'detach')
  await settle()
  assert.equal(app.context.location.hash, hash)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not open a window: Error: denied')
})

test('detaching a worktree leaves a page visited while opening it in place', async () => {
  const { app } = await worktreeApp()
  let finish
  app.window.__TAURI__ = { core: { invoke: () => new Promise((resolve) => { finish = resolve }) } }

  clickCard(app, 'detach')
  navigate(app, '#/sessions')
  finish()
  await settle()
  assert.equal(app.context.location.hash, '#/sessions')
})

test('a worktree root allows its page and its sessions only', async () => {
  const sessions = [sessionAt('s1', `${worktreePath}/`), sessionAt('s3', mainPath)]
  const app = await standaloneApp(worktreeHashOf(worktreePath), sessions)

  for (const hash of [worktreeHashOf(worktreePath), '#/o/r/main/s1', '#/o/r/main/s1/spec.md']) {
    assert.equal(scoped(app, hash), true, hash)
  }
  for (const hash of ['#/', '#/sessions', '#/terminals', worktreeHashOf(mainPath), '#/o/r/main/s3', prsHashOf(mainPath)]) {
    assert.equal(scoped(app, hash), false, hash)
  }
})

test("a session root allows its worktree page and that worktree's sessions", async () => {
  const sessions = [sessionAt('s1', worktreePath), sessionAt('s2', worktreePath), sessionAt('s3', mainPath)]
  const app = standaloneDashboard('#/o/r/main/s1', sessions)

  for (const hash of ['#/o/r/main/s1', '#/o/r/main/s1/spec.md']) {
    assert.equal(scoped(app, hash), true, `${hash} before the first poll`)
  }
  for (const hash of [worktreeHashOf(worktreePath), '#/o/r/main/s2']) {
    assert.equal(scoped(app, hash), false, `${hash} before the first poll`)
  }

  await app.run('poll()')
  await settle()
  for (const hash of ['#/o/r/main/s1', worktreeHashOf(worktreePath), '#/o/r/main/s2']) {
    assert.equal(scoped(app, hash), true, hash)
  }
  for (const hash of ['#/', '#/sessions', worktreeHashOf(mainPath), '#/o/r/main/s3']) {
    assert.equal(scoped(app, hash), false, hash)
  }
})

test("a PRs root allows its repo's worktrees and their sessions", async () => {
  const repos = plain(selectRepos)
  const app = await standaloneApp(prsHashOf(sdkMain), [sessionAt('sa', pathA), sessionAt('sc', pathC)], repos)

  for (const hash of [prsHashOf(sdkMain), worktreeHashOf(pathA), worktreeHashOf(pathB), '#/o/r/main/sa']) {
    assert.equal(scoped(app, hash), true, hash)
  }
  for (const hash of ['#/', prsHashOf(orMain), worktreeHashOf(pathC), '#/o/r/main/sc', worktreeHashOf(fixLogin.path)]) {
    assert.equal(scoped(app, hash), false, hash)
  }

  repos.repos[0].worktrees.push(fixLogin)
  await app.run('poll()')
  await settle()
  assert.equal(scoped(app, worktreeHashOf(fixLogin.path)), true, 'a worktree the repos list after a checkout')
})

test('an out-of-scope hash change restores the shown page', async () => {
  const root = worktreeHashOf(worktreePath)
  const app = await standaloneApp(root, [worktreeSession])

  navigate(app, '#/sessions')
  assert.equal(app.context.location.hash, root)
  assert.equal(app.run('shownHash'), root)

  navigate(app, '#/o/r/main/s1')
  await settle()
  navigate(app, worktreeHashOf(mainPath))
  assert.equal(app.context.location.hash, '#/o/r/main/s1')
  assert.equal(app.run('shownHash'), '#/o/r/main/s1')
})

test('a back key on the root page does nothing', async () => {
  for (const [root, repos] of [[worktreeHashOf(worktreePath), worktreeRepos], ['#/o/r/main/s1', worktreeRepos], [prsHashOf(sdkMain), selectRepos]]) {
    for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
      const app = await standaloneApp(root, [worktreeSession], repos)

      assert.ok(press(app, key, modifiers), `${key} is taken on ${root}`)
      assert.equal(app.context.location.hash, root)
    }
  }
})

test('a back key off the root goes back within scope', async () => {
  const worktreeRoot = await standaloneApp(worktreeHashOf(worktreePath), [worktreeSession])
  navigate(worktreeRoot, '#/o/r/main/s1')
  await settle()
  assert.ok(press(worktreeRoot, 'Escape'))
  assert.equal(worktreeRoot.context.location.hash, worktreeHashOf(worktreePath))

  const sessionRoot = await standaloneApp('#/o/r/main/s1', [worktreeSession])
  navigate(sessionRoot, worktreeHashOf(worktreePath))
  assert.ok(press(sessionRoot, 'Escape'))
  assert.equal(sessionRoot.context.location.hash, '#/o/r/main/s1')

  const prsRoot = await standaloneApp(prsHashOf(sdkMain), [], selectRepos)
  navigate(prsRoot, worktreeHashOf(pathA))
  assert.ok(press(prsRoot, 'Escape'))
  assert.equal(prsRoot.context.location.hash, prsHashOf(sdkMain))
})

// Gives `app` a fake app window API whose `setTitle` runs `setTitle`, and returns the titles it sets.
function titleRecorder(app, setTitle = async () => {}) {
  const titles = []
  app.window.__TAURI__ = {
    window: {
      getCurrentWindow: () => ({
        setTitle: (title) => {
          titles.push(title)
          return setTitle(title)
        },
      }),
    },
  }
  return titles
}

// Returns a standalone dashboard opened on `hash` and the titles it sets over one poll.
async function titledApp(hash, sessions = [], repos = worktreeRepos) {
  const app = standaloneDashboard(hash, sessions, repos)
  const titles = titleRecorder(app)
  await app.run('poll()')
  await settle()
  return { app, titles }
}

test('a standalone worktree window names its repo and branch', async () => {
  const unloaded = standaloneDashboard(worktreeHashOf(worktreePath))
  assert.equal(unloaded.run('standaloneTitle()'), null)

  const { titles } = await titledApp(worktreeHashOf(worktreePath))
  assert.deepEqual(titles, ['Jam · r/feature'])

  const detached = plain(worktreeRepos)
  Object.assign(detached.repos[0].worktrees[1], { branch: null, head: 'abc1234' })
  const { titles: detachedTitles } = await titledApp(worktreeHashOf(worktreePath), [], detached)
  assert.deepEqual(detachedTitles, ['Jam · r/abc1234'])
})

test('a standalone session window names its org, repo and branch', async () => {
  const unloaded = standaloneDashboard('#/org/repo/main/session', [session])
  assert.equal(unloaded.run('standaloneTitle()'), null)

  const { titles } = await titledApp('#/org/repo/main/session', [session])
  assert.deepEqual(titles, ['Jam · org/repo/main'])
})

test('a standalone PRs window names its repo', async () => {
  const { titles } = await titledApp(prsHashOf(sdkMain), [], selectRepos)

  assert.deepEqual(titles, ['Jam · sdk PRs'])
})

test('the title is set once per change', async () => {
  const repos = plain(worktreeRepos)
  const { app, titles } = await titledApp(worktreeHashOf(worktreePath), [], repos)

  await app.run('poll()')
  await settle()
  assert.deepEqual(titles, ['Jam · r/feature'])

  repos.repos[0].worktrees[1].branch = 'renamed'
  await app.run('poll()')
  await settle()
  assert.deepEqual(titles, ['Jam · r/feature', 'Jam · r/renamed'])
})

test('a failed title change tells the user', async () => {
  const app = standaloneDashboard(worktreeHashOf(worktreePath))
  titleRecorder(app, async () => {
    throw new Error('denied')
  })

  await app.run('poll()')
  await settle()
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not set the window title: Error: denied')
})

test('the main window never sets a title', async () => {
  const app = dashboard({
    hash: worktreeHashOf(worktreePath),
    fetcher: (url) => {
      if (url === 'api/repos') return response(JSON.stringify(worktreeRepos))
      if (url === 'api/sessions') return sessionsReply([])
      return response('', false, { status: 404 })
    },
  })
  const titles = titleRecorder(app)

  await app.run('poll()')
  await settle()
  assert.equal(app.run('standaloneTitle()'), null)
  assert.deepEqual(titles, [])
})

test('Cmd+click on a sessions page row opens the session in a new app window', async () => {
  const app = await reposApp(worktreeRepos, [worktreeSession], '#/sessions')
  const opened = appOpener(app)

  assert.ok(clickRow(app, 'card', '#/o/r/main/s1', { metaKey: true }, 'list'))
  await settle()

  assert.deepEqual(opened, ['#/o/r/main/s1'])
  assert.equal(app.context.location.hash, '#/sessions')
})

// Clicks the bar item link `href` with `modifiers`, and returns true when the page prevented the default.
function clickBarItem(app, href, modifiers = {}) {
  let prevented = false
  const link = { getAttribute: (name) => (name === 'href' ? href : null) }
  const target = { closest: (selector) => (selector.includes('a.bar-item') ? link : null) }

  app.elements.get('worktree-bar').dispatch('click', { target, ...modifiers, preventDefault: () => { prevented = true } })
  return prevented
}

test('Cmd+click on a bar item opens it in a new app window', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  const opened = appOpener(app)

  assert.equal(clickBarItem(app, '#/o/r/main/s1'), false)
  assert.ok(clickBarItem(app, '#/o/r/main/s1', { metaKey: true }))
  await settle()

  assert.deepEqual(opened, ['#/o/r/main/s1'])
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
})

test('Cmd+click in a standalone window opens another window and stays put', async () => {
  const root = worktreeHashOf(worktreePath)
  const app = await standaloneApp(root, [worktreeSession])
  const opened = appOpener(app)

  assert.ok(clickBarItem(app, '#/o/r/main/s1', { metaKey: true }))
  await settle()

  assert.deepEqual(opened, ['#/o/r/main/s1'])
  assert.equal(app.context.location.hash, root)
})

// Returns a dashboard on the page of `shown`, a session `s1` of `o/r`, after one poll of an api that answers `api/repos` with `repos`.
async function sessionPageApp(shown = worktreeSession, repos = worktreeRepos) {
  const app = await reposApp(repos, [shown], '#/o/r/main/s1')
  await settle()
  return app
}

test('o, i and e on a session item post to the worktree open route', async () => {
  const { app, posts, sockets } = await sessionItemApp()

  for (const key of ['o', 'i', 'e']) assert.ok(press(app, key), `${key} is taken`)
  await settle()
  assert.deepEqual(posts, ['github', 'intellij', 'zed'].map((tool) => `api/open/${tool}?path=${encodedWorktree}`))
  assert.equal(app.context.location.hash, '#/o/r/main/s1')
  assert.equal(sockets.length, 0)
})

test('c on a session item does nothing', async () => {
  const { app, posts } = await sessionItemApp()

  assert.equal(press(app, 'c'), false)
  await settle()
  assert.equal(app.context.location.hash, '#/o/r/main/s1')
  assert.deepEqual(posts, [])
})

test('a, w and the arrows still act on a session item', async () => {
  const api = fileApi({ 'walk.json': walkJson, 'overview.md': '# Overview\n', 'steps/01.md': '## 1\n' })
  const app = await openSession(api, { ...walkSession('walk'), worktree: worktreePath })
  app.context.repos = worktreeRepos
  app.run('reposState = repos')
  app.context.messages = []
  app.run('ask = async (message) => { messages.push(message); return false }')

  assert.ok(press(app, 'a'))
  await settle()
  assert.deepEqual(app.context.messages, ['Archive o/r/main/s? The dashboard will no longer list it.'])

  assert.equal(app.run('sessionView.selected'), 0)
  assert.ok(press(app, 'ArrowDown'))
  assert.equal(app.run('sessionView.selected'), 1)
  app.window.dispatch('hashchange')
  await settle()

  assert.ok(press(app, 'w'))
  assert.equal(app.run('editing.path'), 'steps/01.md')
  assert.equal(editor(app).tagName, 'TEXTAREA')
})

test('the session strip button archives the session', async () => {
  const app = await sessionPageApp()
  app.context.messages = []
  app.run('ask = async (message) => { messages.push(message); return false }')

  const button = { dataset: { action: 'archive' }, closest: () => ({ dataset: { dir: worktreeSession.dir } }) }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('session-card').dispatch('click', { target })
  await settle()
  assert.deepEqual(app.context.messages, ['Archive o/r/main/s1? The dashboard will no longer list it.'])
})

test('o, i and e on a session item without a listed worktree post to the session open route', async () => {
  for (const worktree of [undefined, '', '/s/o/r/gone', mainPath]) {
    const { app, posts, sockets } = await sessionItemApp([{ ...worktreeSession, worktree }], () => response(JSON.stringify(linkedRepos)))

    for (const key of ['o', 'i', 'e']) assert.ok(press(app, key), `${key} on worktree ${JSON.stringify(worktree)}`)
    await settle()
    assert.deepEqual(posts, ['github', 'intellij', 'zed'].map((tool) => `api/open/${tool}/o/r/main/s1`))
    assert.equal(app.context.location.hash, '#/o/r/main/s1')
    assert.equal(sockets.length, 0)
  }
})

test('g and t on a session item without a listed worktree tell the user', async () => {
  for (const worktree of [undefined, '', '/s/o/r/gone', mainPath]) {
    const { app, posts, sockets } = await sessionItemApp([{ ...worktreeSession, worktree }], () => response(JSON.stringify(linkedRepos)))

    for (const key of ['g', 't']) {
      assert.ok(press(app, key), `${key} on worktree ${JSON.stringify(worktree)}`)
      assert.equal(app.elements.get('modal').open, true)
      assert.equal(app.elements.get('modal-text').textContent, 'Terminals open only in a linked worktree.')
      closeModal(app, '')
    }

    await settle()
    assert.equal(app.context.location.hash, '#/o/r/main/s1')
    assert.deepEqual(posts, [])
    assert.equal(sockets.length, 0)
  }
})

test('the card of a main checkout page names the checkout', async () => {
  const { app } = await sessionItemApp([{ ...worktreeSession, worktree: mainPath }], () => response(JSON.stringify(linkedRepos)))
  const card = () => app.elements.get('worktree-card').innerHTML

  assert.equal(textOf(card()), 'o/r o/r/main main checkout')
  assert.doesNotMatch(card(), /data-action/)

  navigate(app, worktreeHashOf(mainPath))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(mainPath)])
  assert.equal(textOf(card()), 'o/r o/r/main main checkout')
})

test('t in a standalone session window selects the Terminal item', async () => {
  const app = await standaloneApp('#/o/r/main/s1', [worktreeSession])
  terminalStubs(app)

  assert.ok(press(app, 't'))
  app.window.dispatch('hashchange')
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('the worktree keys on a session with a trailing slash act on the listed worktree', async () => {
  const sessions = [{ ...worktreeSession, worktree: `${worktreePath}/` }]
  const { app, posts } = await sessionItemApp(sessions, () => response(JSON.stringify(linkedRepos)))

  assert.ok(press(app, 'o'))
  await settle()
  assert.deepEqual(posts, [`api/open/github?path=${encodedWorktree}`])

  assert.ok(press(app, 'g'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['lazygit*'])
})

test('a standalone session window with a trailing slash has the listed worktree page in scope', async () => {
  const app = await standaloneApp('#/o/r/main/s1', [sessionAt('s1', `${worktreePath}/`)], linkedRepos)

  assert.deepEqual(plain(app.run('standaloneWorktrees()')), [worktreePath])
  assert.equal(scoped(app, worktreeHashOf(worktreePath)), true)
})

test('the session strip shows the archive button with its key', async () => {
  const app = await sessionPageApp()

  assert.match(app.elements.get('session-card').innerHTML, /data-action="archive" title="Archive \(a\)"/)
  assert.doesNotMatch(app.elements.get('session-card').innerHTML, /data-action="worktree"/)
})

// Archives `archived` in `app` once the user confirms, and fires the hash change the archive makes.
async function archive(app, archived) {
  const shown = app.context.location.hash
  app.context.archived = archived
  app.run('ask = async () => true')

  await app.run('archiveSession(archived)')
  if (app.context.location.hash !== shown) app.window.dispatch('hashchange')
  await settle()
}

test('archiving a session in a standalone window shows the root', async () => {
  for (const [root, archived] of [[worktreeHashOf(worktreePath), worktreeSession], ['#/o/r/main/s1', otherSession]]) {
    const app = await standaloneApp(root, [worktreeSession, otherSession])
    navigate(app, `#/o/r/main/${archived.id}`)
    await settle()

    await archive(app, archived)
    assert.equal(app.context.location.hash, root)
  }
})

test('archiving the root session of a standalone window closes it', async () => {
  const root = '#/o/r/main/s1'
  const app = await standaloneApp(root, [worktreeSession])
  const closes = appWindow(app)

  await archive(app, worktreeSession)
  assert.equal(closes.count, 1)
  assert.equal(app.context.location.hash, root)
})

test('a back key on a missing session page of a standalone window goes to the root', async () => {
  for (const [root, id] of [[worktreeHashOf(worktreePath), 's1'], ['#/o/r/main/s1', 's2']]) {
    for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
      const sessions = [sessionAt('s1', worktreePath), sessionAt('s2', worktreePath)]
      const app = await standaloneApp(root, sessions)
      navigate(app, `#/o/r/main/${id}`)
      await settle()

      sessions.splice(sessions.findIndex((s) => s.id === id), 1)
      await app.run('poll()')
      await settle()
      assert.equal(pageText(app), 'session not found')

      assert.ok(press(app, key, modifiers), `${key} is taken on ${id}`)
      assert.equal(app.context.location.hash, root)
    }
  }
})

test('archiving a session in the main window shows its worktree page', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  navigate(app, '#/o/r/main/s1')
  await settle()
  const closes = appWindow(app)

  await archive(app, worktreeSession)
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(closes.count, 0)
})

const newWorktree = { path: '/s/akka/sdk/fix-it', branch: 'octonato/fix-it', head: null }

// Returns the `api/worktrees` URL of `name` for `sdkMain`.
const newWorktreeUrl = (name) => `api/worktrees?path=%2Fs%2Fakka%2Fsdk%2Fmain&name=${encodeURIComponent(name)}`

// Returns a dashboard on `hash` after one poll, with the POST URLs and the pending `api/worktrees` replies.
async function newWorktreeApp(hash) {
  const posts = []
  const creates = []
  const repos = plain(selectRepos)
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') {
        posts.push(url)
        return new Promise((resolve) => creates.push(resolve))
      }
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply([])
      if (url.startsWith('api/prs?path=')) return prsJson(prList)()
      return response('', false, { status: 404 })
    },
  })
  app.context.location.hash = hash
  await app.run('poll()')
  await settle()
  return { app, posts, creates, repos }
}

// Types `name` into the open modal and presses Enter in its text field.
function enterName(app, name) {
  const input = app.elements.get('modal-input')
  input.value = name

  let prevented = false
  input.dispatch('keydown', { key: 'Enter', preventDefault: () => { prevented = true } })
  assert.ok(prevented)
}

// Answers the first pending creation with the path of `newWorktree`, which `api/repos` lists from then on, and waits for the board.
async function finishCreate({ creates, repos }) {
  repos.repos[0].worktrees.push(newWorktree)
  creates.shift()(response(JSON.stringify({ path: newWorktree.path })))
  await settle()
}

test('b on a main row asks for a name, creates the worktree and opens its page', async () => {
  const stubs = await newWorktreeApp('#/')
  const { app, posts } = stubs
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), sdkMain)

  assert.ok(press(app, 'b'))
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-input').hidden, false)
  assert.equal(app.elements.get('modal-input').focused, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Name of the new worktree of akka/sdk')

  enterName(app, ' fix it ')
  await settle()
  assert.deepEqual(posts, [newWorktreeUrl('fix it')])
  assert.equal(app.context.location.hash, '#/')

  await finishCreate(stubs)
  assert.equal(app.context.location.hash, worktreeHashOf(newWorktree.path))
  assert.equal(app.run('creatingWorktree'), false)
})

test('b and the card button on the PRs page create a worktree', async () => {
  for (const act of [(app) => press(app, 'b'), (app) => clickCardButton(app, 'new-worktree')]) {
    const stubs = await newWorktreeApp(prsHashOf(sdkMain))
    const { app, posts } = stubs

    act(app)
    assert.equal(app.elements.get('modal').open, true)
    enterName(app, 'fix-it')
    await settle()
    assert.deepEqual(posts, [newWorktreeUrl('fix-it')])

    await finishCreate(stubs)
    assert.equal(app.context.location.hash, worktreeHashOf(newWorktree.path))
  }
})

// Clicks the card button of `action` on the PRs page.
function clickCardButton(app, action) {
  const button = { dataset: { action } }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('pr-card').dispatch('click', { target })
}

test('the PRs page card holds the create worktree button', async () => {
  const { app } = await newWorktreeApp(prsHashOf(sdkMain))

  assert.match(app.elements.get('pr-card').innerHTML, /data-action="new-worktree" title="Create a worktree \(b\)"/)
})

test('a cancelled or blank name creates nothing', async () => {
  for (const close of [(app) => closeModal(app, 'cancel'), (app) => closeModal(app, ''), (app) => enterName(app, '   ')]) {
    const { app, posts } = await newWorktreeApp(prsHashOf(sdkMain))

    assert.ok(press(app, 'b'))
    close(app)
    await settle()
    assert.deepEqual(posts, [])
    assert.equal(app.run('creatingWorktree'), false)
    assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  }
})

test('b does nothing on a worktree row', async () => {
  const { app, posts } = await newWorktreeApp('#/')
  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathA)

  assert.equal(press(app, 'b'), false)
  assert.notEqual(app.elements.get('modal').open, true)
  assert.deepEqual(posts, [])
})

test('a failed creation tells the error and stays on the page', async () => {
  const { app, creates } = await newWorktreeApp(prsHashOf(sdkMain))

  press(app, 'b')
  enterName(app, 'taken')
  await settle()
  creates.shift()(response('Branch octonato/taken already exists', false, { status: 409 }))
  await settle()

  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-input').hidden, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not create the worktree taken: Branch octonato/taken already exists')
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  assert.equal(app.run('creatingWorktree'), false)
})

test('leaving the page during a creation stays where the user went', async () => {
  const stubs = await newWorktreeApp(prsHashOf(sdkMain))
  const { app } = stubs

  press(app, 'b')
  enterName(app, 'fix-it')
  await settle()
  navigate(app, '#/sessions')
  await finishCreate(stubs)

  assert.equal(app.context.location.hash, '#/sessions')
})

test('archiving a session from the Sessions tab stays there', async () => {
  const { app } = await sessionsListApp()

  await archive(app, worktreeSession)
  assert.equal(app.context.location.hash, '#/sessions')
})

// The `api/worktrees/remove` URL of `pathA`.
const removeAlphaUrl = `api/worktrees/remove?path=${encodeURIComponent(pathA)}`

// The confirm question for the removal of `pathA`.
const removeAlphaQuestion = 'Remove the worktree akka/sdk/alpha? Its terminals close. Git deletes the branch alpha when it is merged.'

// Returns a dashboard on `hash` after one poll of an api that lists `selectRepos` and `sessions`, with fake terminals, the POST URLs and the pending `api/worktrees/remove` replies.
async function removeWorktreeApp(hash, sessions = [], search = '') {
  const posts = []
  const removals = []
  const repos = plain(selectRepos)
  const app = dashboard({
    search,
    hash,
    fetcher: (url, options) => {
      if (options?.method === 'POST') {
        posts.push(url)
        if (url.startsWith('api/worktrees/remove')) return new Promise((resolve) => removals.push(resolve))
        return response('', true, { status: 204 })
      }
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
  const stubs = terminalStubs(app)
  app.context.location.hash = hash
  await app.run('poll()')
  await settle()
  return { app, posts, removals, repos, ...stubs }
}

// Confirms the open removal question and waits for the POST.
async function confirmRemoval(app) {
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, removeAlphaQuestion)
  assert.equal(app.elements.get('modal-input').hidden, true)

  closeModal(app, 'ok')
  await settle()
}

// Answers the first pending removal with success, after which `api/repos` no longer lists `pathA`, and waits for the board.
async function finishRemoval({ removals, repos }) {
  repos.repos[0].worktrees = repos.repos[0].worktrees.filter((worktree) => worktree.path !== pathA)
  removals.shift()(response('', true, { status: 204 }))
  await settle()
}

test('d on a worktree row asks, removes the worktree and selects its main row', async () => {
  const stubs = await removeWorktreeApp('#/')
  const { app, posts } = stubs
  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathA)

  assert.ok(press(app, 'd'))
  await confirmRemoval(app)
  assert.deepEqual(posts, [removeAlphaUrl])
  assert.equal(app.run('removingWorktree'), true)

  await finishRemoval(stubs)
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), sdkMain)
  assert.equal(app.run('removingWorktree'), false)
  assert.doesNotMatch(app.elements.get('repo-list').innerHTML, /alpha/)
})

test('d on a worktree page removes the worktree, drops its tabs and returns to the repos page', async () => {
  const stubs = await removeWorktreeApp(worktreeHashOf(pathA))
  const { app, posts, sockets } = stubs
  clickTab(app, 'add')
  assert.equal(tabsOf(app, pathA).length, 1)

  assert.ok(press(app, 'd'))
  await confirmRemoval(app)
  await finishRemoval(stubs)

  assert.deepEqual(posts, [removeAlphaUrl])
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), sdkMain)
  assert.equal(app.run(`worktreeTerms.has(${JSON.stringify(pathA)})`), false)
  assert.equal(sockets[0].closed, true)
})

test('d on a session item removes its worktree and returns to the repos page', async () => {
  const stubs = await removeWorktreeApp('#/o/r/main/s1', [sessionAt('s1', pathA)])
  const { app, posts } = stubs

  assert.ok(press(app, 'd'))
  await confirmRemoval(app)
  await finishRemoval(stubs)

  assert.deepEqual(posts, [removeAlphaUrl])
  assert.equal(app.context.location.hash, '#/')
})

test('d does nothing on a main row or a session without a listed worktree', async () => {
  const main = await removeWorktreeApp('#/')
  press(main.app, 'ArrowDown')
  assert.equal(main.app.run('selectedWorktree'), sdkMain)
  assert.equal(press(main.app, 'd'), false)
  assert.notEqual(main.app.elements.get('modal').open, true)

  const unlisted = await removeWorktreeApp('#/o/r/main/s1', [sessionAt('s1', '/s/elsewhere')])
  assert.equal(press(unlisted.app, 'd'), false)
  assert.notEqual(unlisted.app.elements.get('modal').open, true)
  assert.deepEqual([...main.posts, ...unlisted.posts], [])
})

test('a cancelled removal posts nothing', async () => {
  const { app, posts } = await removeWorktreeApp(worktreeHashOf(pathA))

  press(app, 'd')
  closeModal(app, 'cancel')
  await settle()

  assert.deepEqual(posts, [])
  assert.equal(app.run('removingWorktree'), false)
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
})

test('a second d during a removal asks nothing', async () => {
  const stubs = await removeWorktreeApp(worktreeHashOf(pathA))
  const { app, posts } = stubs

  press(app, 'd')
  await confirmRemoval(app)
  press(app, 'd')
  assert.notEqual(app.elements.get('modal').open, true)

  await finishRemoval(stubs)
  assert.deepEqual(posts, [removeAlphaUrl])
})

test('a failed removal tells the error and stays on the page', async () => {
  const { app, removals } = await removeWorktreeApp(worktreeHashOf(pathA))

  press(app, 'd')
  await confirmRemoval(app)
  removals.shift()(response('fatal: contains modified or untracked files', false, { status: 409 }))
  await settle()

  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Could not remove the worktree akka/sdk/alpha: fatal: contains modified or untracked files')
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
  assert.equal(app.run('removingWorktree'), false)
})

test('leaving the page during a removal stays where the user went', async () => {
  const stubs = await removeWorktreeApp(worktreeHashOf(pathA))
  const { app } = stubs

  press(app, 'd')
  await confirmRemoval(app)
  navigate(app, '#/sessions')
  await finishRemoval(stubs)

  assert.equal(app.context.location.hash, '#/sessions')
})

test('removing the worktree of a standalone window closes it', async () => {
  const stubs = await removeWorktreeApp(worktreeHashOf(pathA), [], '?standalone')
  const { app } = stubs
  const closes = appWindow(app)

  press(app, 'd')
  await confirmRemoval(app)
  await finishRemoval(stubs)

  assert.equal(closes.count, 1)
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
})

const jumpDialog = (app) => app.elements.get('jump')
const jumpText = (app) => textOf(app.elements.get('jump-list').innerHTML)
const jumpTabsText = (app) => textOf(app.elements.get('jump-tabs').innerHTML)

// Returns the label of each tab the pop-up marks active.
const activeJumpTab = (app) => [...app.elements.get('jump-tabs').innerHTML.matchAll(/aria-selected="true">([^<]*)</g)].map((match) => match[1])

// Returns the text of each selected row of the pop-up.
const selectedJumpText = (app) => [...app.elements.get('jump-list').innerHTML.matchAll(/<div class="jump-row selected"[^>]*>([\s\S]*?)<\/div>/g)]
  .map((match) => textOf(match[1]))

const shift = { metaKey: true, shiftKey: true }

// Returns the paths of the rows the pop-up shows.
const jumpPaths = (app) => plain(app.run('shownJumpRows().map((row) => row.path)'))

// Types `text` into the field of the pop-up.
function typeJump(app, text) {
  const field = app.elements.get('jump-input')
  field.value = text
  field.dispatch('input')
}

// Clicks the pop-up tab label of `tab`.
function clickJumpTab(app, tab) {
  const button = { dataset: { tab } }
  const target = { closest: (selector) => (selector === 'button[data-tab]' ? button : null) }
  app.elements.get('jump-tabs').dispatch('click', { target })
}

// The repos of `selectRepos` and a checkout with an error.
const jumpRepos = {
  error: null,
  repos: [
    ...selectRepos.repos,
    { org: 'x', repo: 'broken', path: '/s/x/broken', branch: null, error: 'not a git checkout', worktrees: [{ path: '/s/x/wt', branch: 'wt', head: null }] },
  ],
}

test('each shortcut opens the pop-up on its tab', async () => {
  const cases = [
    ['b', shift, 'branches', 'akka/sdk/main o/r/main'],
    ['w', shift, 'worktrees', 'akka/sdk/alpha akka/sdk/beta o/r/gamma'],
    ['t', shift, 'terminals', 'No match.'],
    ['B', shift, 'branches', 'akka/sdk/main o/r/main'],
  ]
  for (const [key, modifiers, tab, text] of cases) {
    const app = await reposApp(jumpRepos)

    assert.ok(press(app, key, modifiers))
    assert.equal(jumpDialog(app).open, true)
    assert.equal(app.elements.get('jump-input').focused, true)
    assert.equal(app.run('jump.tab'), tab)
    assert.equal(jumpText(app), text)
    assert.equal(jumpTabsText(app), 'Branches Worktrees Terminals')
    assert.deepEqual(activeJumpTab(app), [plain(app.run('JUMP_TABS')).find((entry) => entry.tab === tab).label])
  }
})

test('jumpRows lists each main checkout, then its worktrees', async () => {
  const app = await reposApp(jumpRepos)
  assert.deepEqual(plain(app.run('jumpRows()')), [
    { path: sdkMain, kind: 'branch', label: 'akka/sdk/main', branch: 'main', hash: prsHashOf(sdkMain), terminals: false, state: null },
    { path: pathA, kind: 'worktree', label: 'akka/sdk/alpha', branch: 'alpha', hash: worktreeHashOf(pathA), terminals: false, state: null },
    { path: pathB, kind: 'worktree', label: 'akka/sdk/beta', branch: 'beta', hash: worktreeHashOf(pathB), terminals: false, state: null },
    { path: orMain, kind: 'branch', label: 'o/r/main', branch: 'main', hash: prsHashOf(orMain), terminals: false, state: null },
    { path: pathC, kind: 'worktree', label: 'o/r/gamma', branch: 'gamma', hash: worktreeHashOf(pathC), terminals: false, state: null },
  ])
})

test('Cmd+B opens no pop-up', async () => {
  const app = await reposApp(selectRepos)
  assert.equal(press(app, 'b', cmd), false)
  assert.equal(app.run('jump'), null)
  assert.notEqual(jumpDialog(app).open, true)
})

test('Cmd+K with any modifiers and the tab shortcuts with Ctrl or Alt open no pop-up', async () => {
  const app = await reposApp(selectRepos)

  for (const modifiers of [{}, { shiftKey: true }, { ctrlKey: true }, { altKey: true }]) {
    press(app, 'k', { ...cmd, ...modifiers })
    assert.equal(app.run('jump'), null)
  }

  for (const key of ['b', 'w', 't']) {
    for (const modifiers of [{ ctrlKey: true }, { altKey: true }]) {
      press(app, key, { ...shift, ...modifiers })
      assert.equal(app.run('jump'), null)
    }
  }
  assert.notEqual(jumpDialog(app).open, true)
})

test('typing in the pop-up filters its rows and selects the first one', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'w', shift)
  for (let i = 0; i < 2; i += 1) press(app, 'ArrowDown')
  assert.equal(app.run('jump.selected'), pathC)

  typeJump(app, 'AKKA')
  assert.deepEqual(jumpPaths(app), [pathA, pathB])
  assert.equal(app.run('jump.selected'), pathA)
  assert.equal(jumpText(app), 'akka/sdk/alpha akka/sdk/beta')

  typeJump(app, 'sdk  ETA')
  assert.deepEqual(jumpPaths(app), [pathB])
  assert.equal(app.run('jump.selected'), pathB)

  typeJump(app, 'nothing')
  assert.deepEqual(jumpPaths(app), [])
  assert.equal(app.run('jump.selected'), null)
  assert.equal(jumpText(app), 'No match.')
})

test('arrows move the pop-up selection and stop at the ends', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'w', shift)
  assert.equal(app.run('jump.selected'), pathA)
  assert.deepEqual(selectedJumpText(app), ['akka/sdk/alpha'])

  assert.ok(press(app, 'ArrowUp'))
  assert.equal(app.run('jump.selected'), pathA)

  for (const path of [pathB, pathC, pathC]) {
    assert.ok(press(app, 'ArrowDown'))
    assert.equal(app.run('jump.selected'), path)
  }
  assert.deepEqual(selectedJumpText(app), ['o/r/gamma'])

  assert.ok(press(app, 'ArrowUp'))
  assert.equal(app.run('jump.selected'), pathB)
})

test('Enter opens the PRs page of a branch and the worktree page of a worktree', async () => {
  const app = await reposApp(selectRepos)

  press(app, 'b', shift)
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, prsHashOf(sdkMain))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)

  app.window.dispatch('hashchange')
  press(app, 'w', shift)
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('the shortcut of the active tab closes the pop-up; another shortcut switches the tab', async () => {
  const app = await reposApp(selectRepos)

  press(app, 'w', shift)
  press(app, 'ArrowDown')
  assert.equal(app.run('jump.tab'), 'worktrees')
  assert.equal(app.run('jump.selected'), pathB)

  assert.ok(press(app, 'b', shift))
  assert.equal(app.run('jump.tab'), 'branches')
  assert.equal(app.run('jump.selected'), sdkMain)

  assert.ok(press(app, 'b', shift))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('Shift+Cmd+[ and Shift+Cmd+] step through the tabs and wrap', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'b', shift)

  for (const tab of ['worktrees', 'terminals', 'branches']) {
    assert.ok(press(app, ']', shift))
    assert.equal(app.run('jump.tab'), tab)
  }
  assert.ok(press(app, '{', shift))
  assert.equal(app.run('jump.tab'), 'terminals')
  assert.ok(press(app, '}', shift))
  assert.equal(app.run('jump.tab'), 'branches')
  assert.ok(press(app, '[', shift))
  assert.equal(app.run('jump.tab'), 'terminals')
  assert.deepEqual(activeJumpTab(app), ['Terminals'])
})

test('clicking a tab label switches the tab and focuses the field', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'b', shift)
  app.elements.get('jump-input').focused = false

  clickJumpTab(app, 'worktrees')
  assert.equal(app.run('jump.tab'), 'worktrees')
  assert.equal(jumpText(app), 'akka/sdk/alpha akka/sdk/beta o/r/gamma')
  assert.equal(app.elements.get('jump-input').focused, true)
})

test('a tab with no match has no selection and Enter does nothing', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'w', shift)
  typeJump(app, 'alpha')

  press(app, '[', shift)
  assert.equal(app.run('jump.tab'), 'branches')
  assert.equal(jumpText(app), 'No match.')
  assert.equal(app.run('jump.selected'), null)
  assert.equal(press(app, 'Enter'), false)
  assert.equal(app.context.location.hash, '#/')
})

test('Shift+Cmd+] with the pop-up closed still shows the next terminal tab', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])

  assert.ok(press(app, ']', shift))
  assert.equal(app.run('jump'), null)
  assert.notEqual(jumpDialog(app).open, true)
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])
})

test('Esc and a native close clear the pop-up', async () => {
  const app = await reposApp(selectRepos)

  press(app, 'w', shift)
  assert.ok(press(app, 'Escape'))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
  assert.equal(app.context.location.hash, '#/')

  press(app, 'b', shift)
  jumpDialog(app).close()
  assert.equal(app.run('jump'), null, 'a native Esc clears the pop-up')
})

test('the filter stays when the tab changes and matches the branch name', async () => {
  const { app } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(linkedRepos)))
  press(app, 'w', shift)

  typeJump(app, 'feature')
  assert.equal(jumpText(app), "o/r/work tree's")

  press(app, 'b', shift)
  assert.equal(app.run('jump.filter'), 'feature')
  assert.equal(jumpText(app), 'No match.')
})

test('a worktree with terminal tabs shows ▣ and the Terminals tab lists it', async () => {
  const { app, sockets } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(linkedRepos)))
  clickTab(app, 'add')

  press(app, 't', shift)
  assert.equal(jumpText(app), "o/r/work tree's ▣")
  assert.equal(app.run('jump.selected'), worktreePath)

  press(app, 'w', shift)
  assert.equal(jumpText(app), "o/r/work tree's ▣")

  press(app, 't', shift)
  sockets[0].onclose({ code: 1000, reason: '' })
  app.run('render()')
  assert.equal(jumpText(app), 'No match.')
  assert.equal(app.run('jump.selected'), null)
})

test('a main checkout with terminal tabs shows no ▣ and has no row in Terminals', async () => {
  const { app } = await worktreeApp(mainPath, [sessionAt('s1', mainPath)], () => response(JSON.stringify(linkedRepos)))
  clickTab(app, 'add')
  assert.equal(tabsOf(app, mainPath).length, 1)

  press(app, 'w', shift)
  assert.equal(jumpText(app), "o/r/work tree's")

  press(app, 't', shift)
  assert.equal(jumpText(app), 'No match.')
})

test('the pop-up row and the bar item color their square', async () => {
  let running = [shellOnServer, { ...claudeOnServer, state: 'attention' }]
  const { app } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(linkedRepos)), () => terminalsReply(running))
  const bar = () => app.elements.get('worktree-bar').innerHTML
  const list = () => app.elements.get('jump-list').innerHTML
  press(app, 't', shift)

  assert.match(bar(), /<span class="term-attention" title="needs you">▣<\/span> Terminal<\/a>/)
  assert.match(list(), /<span class="jump-term term-attention" title="needs you">▣<\/span>/)
  assert.equal(barLinks(app)[0].text, '▣ Terminal')
  assert.equal(jumpText(app), "o/r/work tree's ▣")

  running = [{ ...shellOnServer, state: 'idle' }, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.match(bar(), /<span class="term-idle" title="idle">▣<\/span> Terminal<\/a>/)
  assert.match(list(), /<span class="jump-term term-idle" title="idle">▣<\/span>/)

  running = [shellOnServer, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.match(bar(), /<span class="term-busy" title="busy">▣<\/span> Terminal<\/a>/)
  assert.match(list(), /<span class="jump-term term-busy" title="busy">▣<\/span>/)
})

test('the header and pop-up Terminals tabs show ⚠ while a tab needs attention', async () => {
  let running = [shellOnServer, { ...claudeOnServer, state: 'attention' }]
  const { app } = await worktreeApp(worktreePath, [], undefined, () => terminalsReply(running))
  const alert = '<span class="term-attention" title="needs you">⚠</span>'
  const headerText = () => textOf(app.elements.get('tabs').innerHTML)
  press(app, 'b', shift)

  assert.ok(app.elements.get('tabs').innerHTML.includes(`>Terminals ${alert}</a>`))
  assert.ok(app.elements.get('jump-tabs').innerHTML.includes(`>Terminals ${alert}</button>`))
  assert.equal(headerText(), 'Repos Sessions Terminals ⚠')
  assert.equal(jumpTabsText(app), 'Branches Worktrees Terminals ⚠')

  running = [shellOnServer, { ...claudeOnServer, state: 'idle' }]
  await app.run('poll()')
  assert.equal(headerText(), 'Repos Sessions Terminals')
  assert.equal(jumpTabsText(app), 'Branches Worktrees Terminals')
})

test('closing a tab in attention drops the red marks', async () => {
  const { app } = await runningApp([shellOnServer, { ...claudeOnServer, state: 'attention' }])
  const [, claude] = tabsOf(app)
  const parts = ['tabs', 'jump-tabs', 'jump-list', 'worktree-bar', 'term-tabs']
  const marked = () => parts.filter((id) => /term-attention|⚠/.test(app.elements.get(id).innerHTML))
  press(app, 'w', shift)
  assert.deepEqual(marked(), parts)

  clickTab(app, 'close', claude.id)
  app.run('render()')
  assert.deepEqual(marked(), [])

  press(app, 'Escape')
  navigate(app, '#/')
  assert.doesNotMatch(app.elements.get('repo-list').innerHTML, /term-attention/)

  navigate(app, '#/terminals')
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-attention/)
})

test('the pop-up shortcuts open the pop-up from a terminal tab and from the editor', async () => {
  const terminal = { closest: (selector) => (selector === '#term-view' ? {} : null) }
  for (const key of ['b', 'w', 't']) {
    const app = await reposApp(selectRepos, [], worktreeHashOf(pathA))

    assert.ok(press(app, key, shift, terminal))
    assert.equal(jumpDialog(app).open, true)
  }

  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  press(app, 'w')
  editor(app).value = '# Typed\n'

  assert.ok(press(app, 'w', shift, editor(app)))
  await settle()
  assert.equal(jumpDialog(app).open, true)
  assert.equal(app.run('jump.tab'), 'worktrees')
  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
})

test('keys in the pop-up do not reach the page', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathA)

  press(app, 'w', shift)
  assert.equal(press(app, 'd'), false)
  assert.notEqual(app.elements.get('modal').open, true)
  assert.equal(app.run('removingWorktree'), false)

  assert.ok(press(app, 'ArrowDown'))
  assert.ok(press(app, 'ArrowDown'))
  assert.equal(app.run('jump.selected'), pathC)
  assert.equal(app.run('selectedWorktree'), pathA)

  assert.equal(press(app, '?', { shiftKey: true }), false)
  assert.notEqual(shortcutsDialog(app).open, true)

  const { app: page, sockets } = await worktreeApp()
  press(page, 'w', shift)
  assert.equal(press(page, 't', cmd), false)
  assert.equal(sockets.length, 0)
})

test('the pop-up shortcuts do nothing while the modal is open', async () => {
  const app = await reposApp(selectRepos)

  app.elements.get('modal').open = true
  assert.equal(press(app, 'w', shift), false)
  assert.equal(app.run('jump'), null)
  assert.notEqual(jumpDialog(app).open, true)

  app.elements.get('modal').open = false
  press(app, '?', { shiftKey: true })
  assert.equal(press(app, 'b', shift), false)
  assert.equal(app.run('jump'), null)
  assert.notEqual(jumpDialog(app).open, true)
})

test('the modal above the pop-up keeps its keys', async () => {
  const app = await reposApp(selectRepos)
  const opened = appOpener(app)

  press(app, 'b', shift)
  app.run('tell("Could not refresh.")')
  assert.equal(app.elements.get('modal').open, true)

  assert.equal(press(app, 'Enter'), false)
  assert.equal(press(app, 'Enter', cmd), false)
  await settle()
  assert.equal(app.context.location.hash, '#/')
  assert.deepEqual(opened, [])

  assert.equal(press(app, 'Escape'), false)
  assert.equal(press(app, 'b', shift), false)
  assert.equal(app.run('jump.tab'), 'branches')
  assert.equal(jumpDialog(app).open, true)

  closeModal(app, '')
  assert.ok(press(app, 'Escape'))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('Enter on the discard prompt above the pop-up opens no row', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const closes = appWindow(app)
  const href = app.context.location.hash
  app.context.repos = selectRepos
  app.run('reposState = repos')

  press(app, 'w')
  editor(app).value = '# Typed\n'
  press(app, 'b', shift, editor(app))
  assert.equal(app.run('onCloseRequested()'), false)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  assert.equal(press(app, 'Enter'), false)
  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, href)
  assert.equal(closes.count, 0)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.equal(jumpDialog(app).open, true)
})

test('the pop-up is empty and Enter does nothing before the repos load', () => {
  const app = dashboard()
  app.context.location.hash = '#/'

  assert.ok(press(app, 'b', shift))
  assert.equal(jumpText(app), 'No match.')
  assert.equal(app.run('jump.selected'), null)

  assert.equal(press(app, 'Enter'), false)
  assert.equal(app.context.location.hash, '#/')
  assert.equal(jumpDialog(app).open, true)
})

test('a poll keeps the pop-up filter and selection', async () => {
  const repos = plain(selectRepos)
  const app = await reposApp(repos)
  press(app, 'w', shift)
  typeJump(app, 'akka')
  press(app, 'ArrowDown')

  await app.run('poll()')
  assert.equal(app.run('jump.filter'), 'akka')
  assert.equal(app.run('jump.selected'), pathB)
  assert.deepEqual(selectedJumpText(app), ['akka/sdk/beta'])

  repos.repos[0].worktrees = repos.repos[0].worktrees.filter((worktree) => worktree.path !== pathB)
  await app.run('poll()')
  assert.equal(app.run('jump.selected'), pathA)
  assert.equal(jumpText(app), 'akka/sdk/alpha')
})

// Returns a counter of the writes to the innerHTML of the element `id`.
function countHtmlWrites(app, id) {
  const el = app.elements.get(id)
  const { get, set } = Object.getOwnPropertyDescriptor(el, 'innerHTML')
  const writes = { count: 0 }
  Object.defineProperty(el, 'innerHTML', {
    configurable: true,
    get,
    set: (value) => {
      writes.count += 1
      set(value)
    },
  })
  return writes
}

test('a poll leaves the pop-up tab strip unwritten; a tab switch and a fresh open draw it', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'b', shift)
  const writes = countHtmlWrites(app, 'jump-tabs')

  await app.run('poll()')
  app.run('render()')
  assert.equal(writes.count, 0)

  press(app, ']', shift)
  assert.equal(writes.count, 1)
  assert.deepEqual(activeJumpTab(app), ['Worktrees'])

  press(app, 'Escape')
  press(app, 'b', shift)
  assert.equal(writes.count, 2)
  assert.deepEqual(activeJumpTab(app), ['Branches'])
})

test('Enter on a worktree row while the editor holds unsaved changes asks to discard them', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  terminalStubs(app)
  const href = app.context.location.hash
  app.context.repos = selectRepos
  app.run('reposState = repos')

  press(app, 'w')
  editor(app).value = '# Typed\n'
  press(app, 'w', shift, editor(app))
  assert.ok(press(app, 'Enter'))
  app.window.dispatch('hashchange')
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, href)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')

  press(app, 'w', shift, editor(app))
  press(app, 'Enter')
  app.window.dispatch('hashchange')
  closeModal(app, 'ok')
  await settle()
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
})

test('Cmd+Enter in the main window opens the row in its own window', async () => {
  const app = await reposApp(selectRepos)
  const opened = appOpener(app)

  press(app, 'w', shift)
  assert.ok(press(app, 'Enter', cmd))
  await settle()

  assert.deepEqual(opened, [worktreeHashOf(pathA)])
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('Enter in a standalone window opens the row in its own window and keeps the page', async () => {
  const root = worktreeHashOf(pathA)
  const app = await standaloneApp(root, [], selectRepos)
  const opened = appOpener(app)

  press(app, 'b', shift)
  assert.ok(press(app, 'Enter'))
  await settle()

  assert.deepEqual(opened, [prsHashOf(sdkMain)])
  assert.equal(app.context.location.hash, root)
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('Cmd+Enter in a browser behaves like Enter', async () => {
  const app = await reposApp(selectRepos)

  press(app, 'w', shift)
  assert.ok(press(app, 'Enter', cmd))

  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
  assert.equal(app.run('jump'), null)
  assert.equal(jumpDialog(app).open, false)
})

test('the shortcuts dialog lists the three pop-up shortcuts', () => {
  const app = dashboard()
  app.context.location.hash = '#/'
  press(app, '?', { shiftKey: true })

  assert.match(shortcutsText(app), /^Everywhere \? Show or hide the shortcuts ⌘ click Open a row or bar item in a new window ⇧⌘B Open the pop-up on Branches ⇧⌘W Open the pop-up on Worktrees ⇧⌘T Open the pop-up on Terminals ⌘= ⌘- Make the text of the shown tab bigger or smaller Pop-up /)
})

test('the shortcuts dialog lists the pop-up tab keys', () => {
  const app = dashboard()
  app.context.location.hash = '#/'
  press(app, '?', { shiftKey: true })

  assert.match(shortcutsText(app), /Pop-up ↑ ↓ Move the selection ⇧⌘\[ ⇧⌘\] Show the previous or next tab Enter Open the selected row ⌘Enter Open the selected row in its own window Esc Close the pop-up Repos /)
})

const githubMain = '/a/main'
const githubWorktree = '/a/feature x+y'

// The repos of a checkout at `githubMain` with the linked worktree `githubWorktree`, as `api/repos` lists them.
const githubRepos = {
  error: null,
  repos: [{ org: 'a', repo: 'r', path: githubMain, error: null, worktrees: [{ path: githubWorktree, branch: 'feature', head: null }] }],
}

// Returns the hash of the GitHub item of `path`.
const githubHashOf = (path) => `#/github/${encodeURIComponent(path)}`

// Returns the dashboard of `worktreeApp(path, sessions)` on the checkouts of `githubRepos`, in the app with a stub `invoke`.
async function githubApp(path, sessions = []) {
  const opened = await worktreeApp(path, sessions, () => response(JSON.stringify(githubRepos)))
  opened.app.window.__TAURI__ = { core: { invoke: async () => {} } }
  opened.app.run('render()')
  return opened
}

test('the GitHub route parses', () => {
  const app = dashboard()

  assert.deepEqual(plain(app.run('parseRoute("#/github/%2Fa%2Ffeature%20x%2By")')), { kind: 'github', path: '/a/feature x+y' })
  assert.equal(app.run(`githubHash(${JSON.stringify(githubWorktree)})`), '#/github/%2Fa%2Ffeature%20x%2By')
})

test('the GitHub item shows in the app on configured checkouts', async () => {
  const sessions = [sessionAt('s1', githubWorktree), sessionAt('s2', githubMain)]
  const { app } = await githubApp(githubWorktree, sessions)

  assert.deepEqual(barLinks(app), [
    { href: githubHashOf(githubWorktree), text: 'GitHub' },
    { href: worktreeHashOf(githubWorktree), text: '▣ Terminal' },
    { href: '#/o/r/main/s1', text: 's1 build implement' },
  ])
  assert.match(app.elements.get('worktree-bar').innerHTML, /<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor"/)

  navigate(app, worktreeHashOf(githubMain))
  assert.deepEqual(barLinks(app), [
    { href: githubHashOf(githubMain), text: 'GitHub' },
    { href: worktreeHashOf(githubMain), text: '▣ Terminal' },
    { href: '#/o/r/main/s2', text: 's2 build implement' },
  ])
})

test('the GitHub item hides outside the app and on unlisted paths', async () => {
  const gone = '/a/gone'
  const sessions = [sessionAt('s1', githubWorktree), sessionAt('s2', gone)]

  const { app: browser } = await worktreeApp(githubWorktree, sessions, () => response(JSON.stringify(githubRepos)))
  assert.deepEqual(barLinks(browser).map(({ text }) => text), ['▣ Terminal', 's1 build implement'])

  const { app } = await githubApp(githubWorktree, sessions)
  navigate(app, '#/o/r/main/s2')
  await settle()
  assert.deepEqual(barLinks(app).map(({ text }) => text), ['▣ Terminal', 's2 build implement'])
  assert.equal(app.run(`hasGithubItem(${JSON.stringify(gone)})`), false)
})

test('the GitHub route shows the worktree page with the GitHub item current and parks the terminal', async () => {
  const { app } = await githubApp(githubWorktree)
  clickTab(app, 'add')
  const [tab] = tabsOf(app, githubWorktree)

  navigate(app, githubHashOf(githubWorktree))
  assert.deepEqual(pageIds(app), ['worktree-card', 'worktree-bar', 'github-view'])
  assert.match(app.elements.get('page').innerHTML, /<div class="session-page github-item">/)
  assert.deepEqual(currentBarLinks(app), [githubHashOf(githubWorktree)])
  assert.deepEqual(app.elements.get('term-store').children, [tab.el])
  assert.match(html, /\.session-page\.github-item \{\s*grid-template-columns: minmax\(0, 1fr\);\s*grid-template-rows: auto auto minmax\(0, 1fr\);\s*\}/)

  app.window.dispatch('resize')
  navigate(app, worktreeHashOf(githubWorktree))
  assert.deepEqual(shownTerms(app), [tab.el])
})

test('a standalone worktree keeps its GitHub route in scope', async () => {
  const root = worktreeHashOf(githubWorktree)

  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const app = await standaloneApp(root, [], githubRepos)
    app.window.__TAURI__ = { core: { invoke: async () => {} } }

    assert.equal(app.run(`inScope(parseRoute(githubHash(${JSON.stringify(githubWorktree)})))`), true)
    assert.equal(scoped(app, githubHashOf(githubMain)), false)
    assert.equal(scoped(app, githubHashOf('/a/other')), false)

    navigate(app, githubHashOf(githubWorktree))
    assert.equal(app.context.location.hash, githubHashOf(githubWorktree))
    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, root)
  }
})

test('a GitHub root allows its worktree page and names its repo and branch', async () => {
  const root = githubHashOf(githubWorktree)
  const { app, titles } = await titledApp(root, [sessionAt('s1', githubWorktree)], githubRepos)

  for (const hash of [root, worktreeHashOf(githubWorktree), '#/o/r/main/s1']) assert.equal(scoped(app, hash), true, hash)
  for (const hash of ['#/', githubHashOf(githubMain), worktreeHashOf(githubMain)]) assert.equal(scoped(app, hash), false, hash)
  assert.deepEqual(titles, ['Jam · r/feature'])
})

test('the bar steps onto the GitHub item', async () => {
  const { app } = await githubApp(githubWorktree, [sessionAt('s1', githubWorktree)])

  // Presses the bar key of `direction`, follows the new hash, and returns it.
  const step = async (direction) => {
    assert.ok(pressBar(app, direction), `${direction} is taken`)
    app.window.dispatch('hashchange')
    await settle()
    return app.context.location.hash
  }

  assert.equal(await step('previous'), githubHashOf(githubWorktree))
  assert.deepEqual(currentBarLinks(app), [githubHashOf(githubWorktree)])
  assert.equal(await step('next'), worktreeHashOf(githubWorktree))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(githubWorktree)])
  assert.equal(await step('next'), '#/o/r/main/s1')
  assert.equal(await step('previous'), worktreeHashOf(githubWorktree))
})

test('opening a worktree with no terminal tab shows its GitHub item', async () => {
  const { app } = await githubApp(githubWorktree)

  navigate(app, '#/')
  navigate(app, worktreeHashOf(githubWorktree))
  assert.equal(app.context.location.hash, githubHashOf(githubWorktree))
  assert.deepEqual(currentBarLinks(app), [githubHashOf(githubWorktree)])

  navigate(app, worktreeHashOf(githubWorktree))
  assert.equal(app.context.location.hash, worktreeHashOf(githubWorktree))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(githubWorktree)])
})

test('opening a worktree with a terminal tab shows its Terminal item', async () => {
  const { app } = await githubApp(githubWorktree)
  clickTab(app, 'add')

  navigate(app, '#/')
  navigate(app, worktreeHashOf(githubWorktree))
  assert.equal(app.context.location.hash, worktreeHashOf(githubWorktree))
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(githubWorktree)])
})

test('opening a worktree with a program shows its Terminal item', async () => {
  const { app } = await githubApp(githubWorktree)
  navigate(app, '#/')

  app.run(`openWorktree(${JSON.stringify(githubWorktree)}, 'lazygit')`)
  app.window.dispatch('hashchange')
  assert.equal(app.context.location.hash, worktreeHashOf(githubWorktree))
  assert.deepEqual(tabsOf(app, githubWorktree).map((tab) => tab.program), ['lazygit'])
  assert.deepEqual(currentBarLinks(app), [worktreeHashOf(githubWorktree)])
})

const githubPr = 'https://github.com/o/r/pull/7'

// The bounds of the GitHub placeholder in the dashboards of `githubViewApp`.
const githubBounds = { x: 0, y: 96, width: 1200, height: 640 }

// Returns the `api/github` reply that carries `url` or `error`.
const githubReply = (url, error = null) => response(JSON.stringify({ url, error }))

// Returns the `github_view_show` call of `url` for `githubWorktree` at `bounds` and `zoom`, in a viewport that ends at the bottom of `bounds`.
const githubShow = (url, bounds = githubBounds, zoom = 1) => ['github_view_show', { path: githubWorktree, url, ...bounds, viewportHeight: bounds.y + bounds.height, zoom }]

const githubPlaceholder = (app) => app.elements.get('github-view').textContent

// Returns a dashboard in the app on the repos page after one poll, the `[command, args]` of each invoke, and the pending `api/github` fetches as `{ url, answer }`.
// Each invoke settles as `invokeReply(command)` does, and `api/repos` lists `repos`.
async function githubViewApp(invokeReply = async () => {}, repos = githubRepos) {
  const invoked = []
  const fetches = []
  const invoke = (command, args) => {
    invoked.push(args === undefined ? [command] : [command, plain(args)])
    return invokeReply(command)
  }

  const app = dashboard({
    pageTop: githubBounds.y,
    pageHeight: githubBounds.height,
    tauri: { core: { invoke } },
    fetcher: (url) => {
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply([])
      if (url.startsWith('api/github?')) return new Promise((answer) => fetches.push({ url, answer }))
      return response('', false, { status: 404 })
    },
  })
  app.context.location.hash = '#/'
  await app.run('poll()')
  return { app, invoked, fetches }
}

// Returns the dashboard of `githubViewApp(invokeReply, repos)` on the GitHub item of `githubWorktree` once its URL fetch answers `url`.
async function shownGithubApp(url = githubPr, invokeReply = undefined, repos = undefined) {
  const opened = await githubViewApp(invokeReply, repos)
  navigate(opened.app, githubHashOf(githubWorktree))
  opened.fetches[0].answer(githubReply(url))
  await settle()
  return opened
}

test('entering the GitHub route shows the view', async () => {
  const { app, invoked, fetches } = await githubViewApp()

  navigate(app, githubHashOf(githubWorktree))
  assert.deepEqual(fetches.map(({ url }) => url), ['api/github?path=%2Fa%2Ffeature%20x%2By'])
  assert.equal(githubPlaceholder(app), 'loading…')
  assert.deepEqual(invoked, [])

  fetches[0].answer(githubReply(githubPr))
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr)])
  assert.equal(githubPlaceholder(app), '')
})

test('a failed fetch shows the error', async () => {
  const { app, invoked, fetches } = await githubViewApp()

  navigate(app, githubHashOf(githubWorktree))
  fetches[0].answer(githubReply(null, 'gh: not logged in'))
  await settle()

  assert.equal(githubPlaceholder(app), 'gh: not logged in')
  assert.deepEqual(invoked, [])
})

test('leaving the route hides the view', async () => {
  const { app, invoked } = await shownGithubApp()

  navigate(app, '#/')
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])

  await app.run('poll()')
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])
})

test('leaving the route hides the view only once the pending show settles', async () => {
  let finishShow
  const invokeReply = async (command) => {
    if (command === 'github_view_show') await new Promise((resolve) => (finishShow = resolve))
  }
  const { app, invoked } = await shownGithubApp(githubPr, invokeReply)

  navigate(app, '#/')
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr)])

  finishShow()
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])
})

test('a worktree gone from the repos hides the view', async () => {
  const repos = { ...githubRepos, repos: [{ ...githubRepos.repos[0] }] }
  const { app, invoked } = await shownGithubApp(githubPr, undefined, repos)

  repos.repos[0].worktrees = []
  await app.run('poll()')
  await settle()
  assert.equal(app.context.location.hash, githubHashOf(githubWorktree))
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])
})

test('pop-ups hide and restore the view', async () => {
  const { app, invoked } = await shownGithubApp()
  const popUps = [
    [() => press(app, 'b', { metaKey: true, shiftKey: true }), () => press(app, 'Escape')],
    [() => press(app, '?', { shiftKey: true }), () => press(app, '?', { shiftKey: true })],
    [() => app.run('tell("Done.")'), () => closeModal(app, 'ok')],
  ]

  for (const [open, close] of popUps) {
    invoked.length = 0
    open()
    await settle()
    assert.deepEqual(invoked, [['github_view_hide']])

    close()
    await settle()
    assert.deepEqual(invoked, [['github_view_hide'], githubShow(githubPr)])
  }
})

test('a late fetch does not show the view under a pop-up', async () => {
  const { app, invoked, fetches } = await githubViewApp()

  navigate(app, githubHashOf(githubWorktree))
  assert.ok(press(app, 'b', { metaKey: true, shiftKey: true }))
  fetches[0].answer(githubReply(githubPr))
  await settle()
  assert.deepEqual(invoked, [])

  assert.ok(press(app, 'Escape'))
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr)])
})

test('the GitHub item of another worktree hides the view while it loads or fails', async () => {
  const other = { path: '/a/other', branch: 'other', head: null }
  const repos = { ...githubRepos, repos: [{ ...githubRepos.repos[0], worktrees: [...githubRepos.repos[0].worktrees, other] }] }
  const { app, invoked, fetches } = await shownGithubApp(githubPr, undefined, repos)

  navigate(app, githubHashOf(other.path))
  await settle()
  assert.deepEqual(fetches.map(({ url }) => url), ['api/github?path=%2Fa%2Ffeature%20x%2By', 'api/github?path=%2Fa%2Fother'])
  assert.equal(githubPlaceholder(app), 'loading…')
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])

  fetches[1].answer(githubReply(null, 'gh: not logged in'))
  await settle()
  assert.equal(githubPlaceholder(app), 'gh: not logged in')
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])
})

test('a show error shows in the placeholder', async () => {
  const error = 'Cannot read githubSignInHosts from /home/me/.jam-factory/config.json: bad entry *okta.com'
  const invokeReply = async (command) => {
    if (command === 'github_view_show') throw error
  }
  const { app, invoked } = await shownGithubApp(githubPr, invokeReply)

  assert.equal(githubPlaceholder(app), error)
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide']])

  await app.run('poll()')
  await settle()
  assert.equal(invoked.length, 2)
})

test('the URL is fetched once per path', async () => {
  const { app, invoked, fetches } = await shownGithubApp()

  navigate(app, worktreeHashOf(githubWorktree))
  navigate(app, githubHashOf(githubWorktree))
  await settle()

  assert.equal(fetches.length, 1)
  assert.deepEqual(invoked, [githubShow(githubPr), ['github_view_hide'], githubShow(githubPr)])
})

test('the view follows its placeholder on resize and font-size change', async () => {
  const { app, invoked } = await shownGithubApp()

  app.window.dispatch('resize')
  await app.run('poll()')
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr)])

  app.geometry.pageHeight = 500
  app.window.dispatch('resize')
  await settle()
  assert.deepEqual(invoked.at(-1), githubShow(githubPr, { ...githubBounds, height: 500 }))

  app.geometry.pageTop = 120
  app.elements.get('font-down').dispatch('click')
  await settle()
  assert.deepEqual(invoked.at(-1), githubShow(githubPr, { ...githubBounds, y: 120, height: 500 }))
  assert.equal(invoked.length, 3)
})

test('a text size step on the GitHub item zooms the view and stores the zoom', async () => {
  const { app, invoked } = await shownGithubApp()

  app.run('stepTextSize(1)')
  await settle()
  assert.deepEqual(invoked, [githubShow(githubPr), githubShow(githubPr, githubBounds, 1.1)])
  assert.equal(app.run('fontSize'), 18)
  assert.equal(app.run("localStorage.getItem('jam-board-github-zoom')"), '1.1')
})
