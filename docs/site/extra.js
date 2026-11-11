// 新增的互动：首屏背景跟时间走、截图上的提醒、关窗口演示、printf 演示、agent 树、深浅色和中英文。
(() => {
  const $ = (s, r = document) => r.querySelector(s), $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const root = document.documentElement;
  const still = () => document.body.classList.contains('still');
  // 系统开了「减少动态效果」：页面上那个动效开关默认关掉，整页都停；想看可以再打开
  const motionSw = $('#motionsw');
  if (motionSw && matchMedia('(prefers-reduced-motion: reduce)').matches && motionSw.getAttribute('aria-checked') === 'true') motionSw.click();
  const save = (k, v) => { try { localStorage.setItem(k, v); } catch (e) {} };
  const load = k => { try { return localStorage.getItem(k); } catch (e) { return null; } };
  const redraw = [];                       // 换语言时要重画的动态文字
  const zh = () => root.lang === 'zh-CN';
  const T = (en, cn) => zh() ? cn : en;

  /* ---------- 首屏：背景可以换白天 / 傍晚 / 夜里，Auto 按访客的时间 ---------- */
  const note = $('#skynote');
  let skyMode = 'day';
  const skyFor = h => (h >= 7 && h < 16) ? 'day' : ((h >= 16 && h < 20) || (h >= 5 && h < 7)) ? 'dusk' : 'night';
  function setSky(mode) {
    skyMode = mode;
    const d = new Date(), sky = mode === 'auto' ? skyFor(d.getHours()) : mode;
    root.dataset.sky = sky;
    $$('.skyctl button').forEach(b => b.classList.toggle('on', b.dataset.sky === mode));
    const hm = String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
    const line = { day: T('agents at work', 'agent 在干活'), dusk: T('close the lid, they keep going', '合上电脑，它们接着跑'), night: T('you sleep, they don\'t', '你睡了，它们还在跑') }[sky];
    if (note) note.textContent = (mode === 'auto' ? hm + ' · ' : '') + line;
  }
  $$('.skyctl button').forEach(b => b.addEventListener('click', () => setSky(b.dataset.sky)));
  setSky('day'); redraw.push(() => setSky(skyMode));   // 默认固定白天，Auto 才按时间换

  /* ---------- 首屏截图上轮流弹出的提醒 ---------- */
  const toasts = $('#toasts');
  if (toasts) {
    const items = [
      ['need', 'Codex', 'fibo', 'Two ways to fix this. Which one should I take?', '有两种修法，用哪一种？'],
      ['ready', 'Claude Code', 'api-server', 'Done. Ready for your review.', '做完了，等你验收。'],
      ['need', 'Grok Build', 'web', 'Native loading="lazy", or an observer?', '用原生 lazy，还是 observer？'],
      ['ready', 'Codex', 'docs', 'Wrote the setup guide. 214 passed.', '安装指南写好了，214 项通过。'],
    ];
    let k = 0, timer = null;
    const show = () => {
      const [kind, who, space, en, cn] = items[k++ % items.length];
      const el = document.createElement('div');
      el.className = 'toast ' + kind;
      el.innerHTML = `<i></i><b>${who} ${kind === 'need' ? T('needs you', '在等你') : T('is ready for review', '等你验收')}</b><small>${space} · ${T(en, cn)}</small>`;
      toasts.replaceChildren(el);
      if (still()) return;
      timer = setTimeout(() => { el.classList.add('out'); timer = setTimeout(show, 1300); }, 3400);
    };
    setTimeout(show, 1400);
    redraw.push(() => { clearTimeout(timer); k = Math.max(0, k - 1); show(); });
  }

  /* ---------- 关掉窗口，它们还在跑 ---------- */
  const demo = $('#closedemo');
  if (demo) {
    const rows = [['', 'Add English UI to settings', 'Claude Code', 627], ['', 'Rate-limit the upload API', 'Codex', 807], ['need', 'Fix flaky reconnect test', 'Codex', 95]];
    const ul = $('#win-rows'), hostN = $('#host-n'), hint = $('#win-hint');
    const mmss = s => String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0');
    let closed = false, reopened = false;
    const draw = () => {
      ul.innerHTML = rows.map(r => `<li class="${r[0]}"><i></i>${r[1]}<small>${r[2]}</small><em>${mmss(r[3])}</em></li>`).join('');
      $('#win-open').textContent = T('Open Fibo again', '重新打开 Fibo');
      hostN.textContent = closed ? T('closed · 3 still running · ', '窗口已关 · 3 个还在跑 · ') + mmss(rows[0][3]) : T('3 terminals running', '3 个终端在跑');
      hint.textContent = closed ? T('The window is gone. Look at the bar: the clock is still running.', '窗口没了。看底下那条：时间还在走。')
        : reopened ? T('Same terminals, same scrollback. Nothing restarted.', '还是那几个终端，内容都在，没有重启。') : T('Click the red button.', '点那个红色按钮。');
    };
    setInterval(() => { rows.forEach(r => r[3]++); draw(); }, 1000);
    $('#win-close').addEventListener('click', () => { closed = true; demo.classList.add('closed'); draw(); });
    $('#win-open').addEventListener('click', () => { closed = false; reopened = true; demo.classList.remove('closed'); draw(); });
    draw(); redraw.push(draw);
  }

  /* ---------- 一行 printf 报状态 ---------- */
  const osc = $('#oscdemo');
  if (osc) {
    const look = {
      working: ['Running', '在跑', '#f5b82e', '#fbf0cc', '#8a6a00'], blocked: ['Needs you', '要你处理', '#f0683e', '#fde4d3', '#b8501a'],
      done: ['Ready for review', '等你验收', '#2fa67a', '#dcf2e6', '#1f7a4f'], error: ['Error', '出错了', '#d9443a', '#fbdcd9', '#a32a22'], idle: ['Stopped', '停下了', '#a9adba', '#eceae4', '#5d6270'],
    };
    let state = 'working';
    const p = $('#osc-p'), code = $('#osc-code'), row = $('#osc-row'), badge = $('#osc-badge'), bar = $('#osc-bar');
    const draw = () => {
      const [en, cn, dot, bg, fg] = look[state], w = state === 'working';
      code.innerHTML = `printf '\\033]7501;state=<b>${state}</b>:app=deploy${w ? `:progress=<b>${p.value}</b>` : ''}\\033\\\\'`;
      $('#osc-pv').textContent = p.value; $('#osc-prog').classList.toggle('off', !w);
      $('i', row).style.background = dot; badge.style.background = bg; badge.style.color = fg;
      badge.textContent = T(en, cn) + (w ? ' · ' + p.value + '%' : '');
      bar.style.width = (w ? p.value : state === 'done' ? 100 : 0) + '%'; bar.style.background = dot;
      $$('#osc-states button').forEach(b => b.classList.toggle('on', b.dataset.s === state));
    };
    $$('#osc-states button').forEach(b => b.addEventListener('click', () => { state = b.dataset.s; draw(); }));
    p.addEventListener('input', draw);
    draw(); redraw.push(draw);
  }

  /* ---------- agent 生 agent：每个跑完一轮的 agent 再起一个 ---------- */
  const tree = $('#tree');
  if (tree) {
    const G = 7, FIB = [1, 1, 2, 3, 5, 8, 13];
    let nodes = [{ id: 0, born: 0, parent: -1 }];
    for (let g = 1; g < G; g++) {
      const add = [];
      nodes.forEach(n => { if (g - n.born >= 2) add.push({ id: nodes.length + add.length, born: g, parent: n.id }); });
      nodes = nodes.concat(add);
    }
    // 泳道：孩子排在父亲下面，越晚出生的越靠近父亲，连线就不会交叉
    const lanes = [];
    (function walk(n) { lanes.push(n.id); nodes.filter(c => c.parent === n.id).sort((a, b) => b.born - a.born).forEach(walk); })(nodes[0]);
    const X = g => 44 + g * 79, Y = id => 62 + lanes.indexOf(id) * 28;
    let cur = 0, timer = null, seen = false, touched = false;
    const out = $('#tree-n'), stepBtn = $('#tree-step');
    function draw() {
      let h = '';
      for (let g = 0; g < G; g++) h += `<text class="cnt${g === cur ? ' now' : ''}" x="${X(g)}" y="26">${g <= cur ? FIB[g] : '·'}</text>`;
      nodes.filter(n => n.born <= cur).forEach(n => {
        const fresh = n.born === cur && cur > 0 ? ' new' : '';
        if (n.parent >= 0) { const px = X(n.born - 1), py = Y(n.parent), cx = X(n.born), cy = Y(n.id); h += `<path class="spawn${fresh}" d="M${px} ${py}C${px + 46} ${py} ${cx - 46} ${cy} ${cx} ${cy}"/>`; }
        if (cur > n.born) h += `<path class="lane" d="M${X(n.born)} ${Y(n.id)}H${X(cur)}"/>`;
      });
      nodes.filter(n => n.born <= cur).forEach(n => {
        for (let g = n.born; g <= cur; g++) {
          const young = g - n.born < 1;
          h += `<circle class="n${young ? ' young' : ''}${g === cur && cur > 0 ? ' new' : ''}" cx="${X(g)}" cy="${Y(n.id)}" r="${young ? 4.5 : 6}"/>`;
        }
      });
      tree.innerHTML = h;
      out.textContent = FIB[cur] + T(FIB[cur] === 1 ? ' agent' : ' agents', ' 个 agent');
      stepBtn.disabled = cur >= G - 1;
    }
    const stop = () => { clearInterval(timer); timer = null; };
    const play = () => { if (timer || still() || touched) return; timer = setInterval(() => { if (cur >= G - 1) return stop(); cur++; draw(); }, 1150); };
    stepBtn.addEventListener('click', () => { touched = true; stop(); if (cur < G - 1) { cur++; draw(); } });
    $('#tree-reset').addEventListener('click', () => { touched = false; stop(); cur = 0; draw(); if (seen) play(); });
    new IntersectionObserver(es => { seen = es[0].isIntersecting; seen ? play() : stop(); }, { threshold: .4 }).observe(tree);
    draw(); redraw.push(draw);
  }

  /* ---------- 30 秒短片：点了才加载、才播放 ---------- */
  const film = $('#film'), filmV = $('#film-v'), filmBtn = $('#filmbtn');
  if (film && filmBtn && film.showModal) {
    filmBtn.addEventListener('click', () => { film.showModal(); filmV.play().catch(() => {}); });
    $('#film-x').addEventListener('click', () => film.close());
    film.addEventListener('click', e => { if (e.target === film) film.close(); });
    film.addEventListener('close', () => filmV.pause());
  }

  /* ---------- 完成动画里的字符表情 ----------
     帧表、时间点和周围的字符照 App 里的写法（mooring-core 的 face.rs、mooring-app 的 complete/cozy.rs）：
     每帧是 [左眼, 嘴, 右眼, 毫秒, 乱码进场]，一轮 3 秒，笑了以后变暖色，最后缩小变绿回到侧栏那一行。 */
  const stage = $('.motion .stage');
  if (stage) {
    const RUN = 3000, PI = Math.PI, TAU = PI * 2, SCR = ['-', '^', 'o', '=', '*', '~', '+', '.', ':'], TW = ['.', '+', '*', '+', '.'];
    const INK = [20, 26, 46], WARM = [214, 118, 24], ROSE = [222, 82, 120], SOFT = [139, 144, 163], GREEN = [47, 163, 107];
    const SHEETS = {
      relief: { open: ' ', frames: [['-', '_', '- ', 420], ['-', '_', '-;', 360], ['-', '.', '-;', 240], ['-', 'o', '- ', 300], ['-', '_', '- ', 120], ['^', '_', '- ', 120], ['^', '—', '^ ', 480], ['-', '—', '- ', 90], ['^', '—', '^ ', 300]], words: [.54, .62], fade: .82, land: .92 },
      wake: { frames: [['=', '_', '=', 1200], ['=', '_', 'o', 240], ['o', '_', 'o', 180], ['-', '_', '-', 90], ['o', '_', 'o', 150], ['^', '—', '^', 360, 1], ['-', '—', '-', 90], ['^', '—', '^', 300]], words: [.66, .72], fade: .84, land: .93 },
      sunrise: { frames: [['-', '_', '-', 1410], ['^', '—', '^', 420, 1], ['*', '—', '*', 120], ['^', '—', '^', 300]], words: [.54, .62], fade: .82, land: .92 },
      heart: { frames: [['-', '_', '-', 1080], ['o', '_', 'o', 180], ['^', '—', '^', 420, 1], ['*', '—', '*', 120], ['^', '—', '^', 300]], words: [.5, .58], fade: .82, land: .92 },
      cat: { open: '=', close: '=', frames: [['-', '.', '-', 780], ['^', '.', '-', 210], ['-', '.', '^', 210], ['-', '.', '-', 180], ['^', '.', '^', 540], ['-', '.', '-', 90], ['^', '.', '^', 300]], words: [.54, .62], fade: .82, land: .92 },
      tea: { open: 'c[', close: ']', frames: [['-', '_', '-', 1380], ['^', '_', '-', 120], ['^', '—', '^', 540], ['-', '—', '-', 120], ['^', '—', '^', 300]], words: [.58, .66], fade: .84, land: .93 },
      hop: { frames: [['-', '_', '-', 540], ['^', '—', '^', 300, 1], ['*', '—', '*', 120], ['^', '—', '^', 300]], words: [.2, .3], wordsOut: [.68, .74], fade: .72, land: .88 },
    };
    Object.values(SHEETS).forEach(s => {
      let clock = 0; const open = s.open || '', close = s.close || '';
      s.beats = s.frames.map(([l, m, r, ms, scr]) => { const b = { at: clock / RUN, settled: (clock + (scr ? Math.min(240, ms) : 0)) / RUN, text: open + l + ' ' + m + ' ' + r + close }; clock += ms; return b; });
      const last = s.beats[s.beats.length - 1].text;
      s.smile = s.beats.find(b => b.text === last).at; s.from = open.length; s.tail = close.length;
    });
    const sl = (t, a, b) => Math.min(1, Math.max(0, (t - a) / (b - a))), eo = x => 1 - (1 - x) ** 4, eio = x => x < .5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2;
    const lerp = (a, b, k) => a + (b - a) * k, bounce = (x, n) => Math.abs(Math.sin(PI * n * x)) * (1 - x), backOut = x => 1 + 2.70158 * (x - 1) ** 3 + 1.70158 * (x - 1) ** 2;
    const mix = (a, b, k) => a.map((v, i) => Math.round(lerp(v, b[i], k)));
    const rng = seed => { let s = seed >>> 0; return () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296; };
    const SHRINK = [1, .76, .56, .4, .28], shrunk = (size, k) => Math.round(size * SHRINK[Math.round(Math.min(1, Math.max(0, k)) * 4)]);
    // t 时刻的脸；乱码进场的那一帧，没落定的字符先换成随机的，从左到右落定
    function faceAt(s, t) {
      let i = 0; s.beats.forEach((b, k) => { if (b.at <= t) i = k; });
      const b = s.beats[i];
      if (t >= b.settled || b.settled <= b.at) return b.text;
      const step = Math.min(5, Math.floor(Math.max(0, t - b.at) / (b.settled - b.at) * 6)), ch = [...b.text], to = ch.length - s.tail, done = Math.floor((to - s.from) * step / 6);
      return ch.map((c, at) => (c === ' ' || at < s.from + done || at >= to) ? c : SCR[(at * 7 + step * 13 + i * 3) % 9]).join('');
    }
    function scene(key, s, t, layer) {
      const W = stage.clientWidth, H = stage.clientHeight, size = 34, side = 14, cx = W / 2, cy = H * .46, tx = 27, ty = 27;
      const g = [], gc = [];
      const put = (x, y, text, sz, col, a, clip) => { if (a > .003) (clip ? gc : g).push(`<span style="left:${x.toFixed(1)}px;top:${y.toFixed(1)}px;font-size:${sz}px;color:rgb(${col});opacity:${Math.min(1, a).toFixed(3)}">${text.replace(/&/g, '&amp;').replace(/</g, '&lt;')}</span>`); };
      const smile = s.smile, tint = t >= smile ? WARM : INK, leaves = s.fade;
      const around = 1 - sl(t, leaves - .03, leaves + .01);
      const joy = bounce(sl(t, smile, smile + .2), 2) * size * .6;
      const float = Math.round(Math.sin(t * 3 * TAU) * size * .05 * sl(t, smile + .1, smile + .16) * (1 - sl(t, leaves - .06, leaves)));
      const flight = k => { const mx = lerp(cx, tx, .5) - 24, my = Math.min(cy, ty) - 6; return [(1 - k) ** 2 * cx + 2 * k * (1 - k) * mx + k * k * tx, (1 - k) ** 2 * cy + 2 * k * (1 - k) * my + k * k * ty]; };
      const face = (lift, clip) => { const k = sl(t, leaves, s.land); if (k >= 1) return; const [x, y] = flight(eio(k)); put(x, y - lift * (1 - k), faceAt(s, t), shrunk(size, eo(k)), mix(tint, GREEN, k), sl(t, .05, .09), clip); };
      const twinkles = (k, seed) => {
        if (k <= 0 || k >= 1) return;
        const r = rng(seed);
        for (let i = 0; i < 6; i++) {
          const ang = -PI * (i + r()) / 5.2 + .25, far = 1.5 + r() * .7, own = sl(k, i * .08, .6 + i * .08);
          if (own > 0 && own < 1) put(cx + Math.cos(ang) * far * size * 1.5, cy + Math.sin(ang) * far * size, TW[Math.min(4, Math.floor(own * 5))], side, WARM, Math.sin(PI * own));
        }
      };
      let clipH = 0;
      if (key === 'relief') {            // 深吸一口气，呼出一串 ~
        for (let i = 0; i < 4; i++) { const k = sl(t, .34 + i * .025, .54 + i * .025); if (k > 0 && k < 1) put(cx + (1.7 + k * 1.6) * size, cy + (.2 - k * .5) * size, '~', side, SOFT, 1 - k); }
        face(joy + float); twinkles(sl(t, smile + .02, smile + .26), 11);
      } else if (key === 'wake') {       // 冒着 z 打瞌睡，睁一只眼，眨一下
        ['z', 'z', 'Z'].forEach((z, i) => { const k = sl(t, .08 + i * .1, .34 + i * .1); if (k > 0 && k < 1) put(cx + (1.5 + 1.1 * k) * size, cy - (.3 + 1.2 * k) * size, z, side, SOFT, Math.sin(PI * k) * (1 - sl(t, .38, .42))); });
        face(bounce(sl(t, .4, .48), 1) * size * .3 + joy + float); twinkles(sl(t, smile + .02, smile + .22), 23);
      } else if (key === 'sunrise') {    // 从一排 ____ 后面升起来，天色变暖，光芒散开
        const sky = eo(sl(t, .08, .5)) * around, horizon = cy + size * .8, leaving = t >= leaves;
        clipH = horizon;
        if (sky > 0) {
          gc.push(`<b style="position:absolute;inset:0;background:linear-gradient(to top,rgba(${WARM},${(.2 * sky).toFixed(3)}),rgba(${WARM},0))"></b>`);
          put(cx, horizon - side * .3, '_'.repeat(Math.min(64, Math.floor(W * .72 / (side * .6)))), side, WARM, sky);
        }
        const rays = eo(sl(t, smile, smile + .12)) * around;
        if (rays > 0) ['-', '\\', '|', '/', '-', '\\', '|', '/'].forEach((ray, i) => { const a = i / 8 * TAU; put(cx + Math.cos(a) * size * 2.3, cy + Math.sin(a) * size * 1.25, ray, side, WARM, rays * (.65 + .35 * Math.sin(t * 30 + i)), !leaving); });
        face(float - lerp(size * 2, 0, eio(sl(t, .08, .46))), !leaving);
      } else if (key === 'heart') {      // 一个 <3 飘过来碰到它，笑了，更多 <3 往上飘
        const k = sl(t, .14, .36);
        if (k > 0 && k < 1) put(cx + Math.sin(k * 13) * .1 * size, cy + lerp(2.2, .8, eio(k)) * size, '<3', side, ROSE, sl(k, 0, .15));
        const r = rng(5);
        for (let i = 0; i < 10; i++) {
          const x = r() * 2 - 1, sway = r() * TAU, start = .42 + r() * .34, at = Math.sign(x) * (2.2 + Math.abs(x) * 1.2), q = sl(t, start, start + .28);
          if (q > 0 && q < 1) put(cx + (at + Math.sin(q * 6 + sway) * .2) * size, cy + (.5 - 2.4 * q) * size, '<3', side, ROSE, Math.min(Math.sin(PI * q), .5) * 2);
        }
        face(joy + float);
      } else if (key === 'cat') {        // 一只只睁开眼，招招手，留下一串 :. 走回去
        for (let i = 0; i < 7; i++) {
          const along = (i + 1) / 8, start = lerp(leaves, s.land, along), shown = sl(t, start, start + .02) * (1 - sl(t, start + .03, start + .08));
          if (shown > 0) { const [x, y] = flight(along); put(x, y + (i % 2 ? .4 : -.4) * side, ':.', side, WARM, shown); }
        }
        const wave = sl(t, smile + .06, smile + .1) * around;
        if (wave > 0) put(cx + size * 2.3, cy - size * .1, Math.floor(t * 16) % 2 ? '\\' : '/', size, WARM, wave);
        face(joy + float);
      } else if (key === 'tea') {        // 茶杯冒着 ~ 热气，暖了，头顶冒出 <3
        const shown = sl(t, .05, .09) * around, steam = sl(t, .12, .24) * shown, flip = Math.floor(t * 10) % 2;
        [['~ ~ ~', ' ~ ~ ~'], [' ~ ~ ~', '~ ~ ~']].forEach((row, i) => put(cx, cy - (size * .85 + side * 1.1 * i), row[flip], side, SOFT, steam));
        put(cx, cy - (size * .85 + side * 2.6), '<3', side, ROSE, Math.min(1, Math.max(0, backOut(sl(t, smile + .04, smile + .12)))) * shown);
        put(cx, cy + size * .72, "'-------'", side, SOFT, shown);
        face(0);
      } else if (key === 'hop' && t < s.land) {   // 高兴起来，一蹦一跳蹦回侧栏那一行
        const going = sl(t, .34, s.land) * 5, i = Math.min(4, Math.floor(going)), f = Math.min(1, going - i), along = (i + eio(f)) / 5;
        const gx = lerp(cx, tx, along), gy = lerp(cy, ty, along);
        if (going > 0 && f < .45) { const k = f / .45; [-1, 1].forEach(d => put(gx + d * size * (.8 + .9 * k), gy + size * .4, '.', side, SOFT, 1 - k)); }
        put(gx, gy - (Math.sin(PI * f) * size * 1.5 * (1 - .13 * i) + joy), faceAt(s, t), shrunk(size, along), mix(tint, GREEN, sl(along, .7, 1)), sl(t, .05, .09));
        twinkles(sl(t, .18, .36), 31);
      }
      // 脸下面那行字，和 App 里一样写「✓ 做完了」
      const words = sl(t, s.words[0], s.words[1]) * (s.wordsOut ? 1 - sl(t, s.wordsOut[0], s.wordsOut[1]) : around);
      put(cx, cy + size * 1.85, T('✓ Done', '✓ 做完了'), 13, GREEN, words);
      const [clip, plain] = layer.querySelectorAll('.glyphs');
      clip.style.height = clipH + 'px'; clip.innerHTML = gc.join(''); plain.innerHTML = g.join('');
      layer.firstChild.classList.toggle('in', t >= s.land);
    }
    let live = null;
    const playFace = (key, fx, demo) => {
      if (live) live();
      const mark = document.createElement('i'), layer = document.createElement('div');
      fx.appendChild(mark);
      layer.className = 'facefx'; layer.setAttribute('aria-hidden', 'true');
      layer.innerHTML = '<div class="siderow"><i></i><span></span></div><div class="glyphs clip"></div><div class="glyphs"></div>';
      $('span', layer).textContent = $('h4', demo).textContent;
      stage.appendChild(layer); demo.classList.add('facing');
      let over = false; const t0 = performance.now();
      const end = live = () => { if (over) return; over = true; layer.remove(); demo.classList.remove('facing'); if (live === end) live = null; };
      (function tick() {
        if (over) return;
        if (!mark.isConnected) return end();          // 换了别的动画，或者演示卡复位了
        const t = (performance.now() - t0) / RUN;
        if (t <= 1) scene(key, SHEETS[key], t, layer);
        requestAnimationFrame(tick);
      })();
      return RUN + 350;
    };
    window.FIBO_FACES = {};
    Object.keys(SHEETS).forEach(k => { window.FIBO_FACES[k] = (fx, demo) => playFace(k, fx, demo); });
  }

  /* ---------- 深色 / 浅色 ---------- */
  const themeBtn = $('#themebtn');
  const setTheme = t => { root.dataset.theme = t; save('fibo-landing-theme', t); };
  setTheme(load('fibo-landing-theme') === 'dark' ? 'dark' : 'light');
  if (themeBtn) themeBtn.addEventListener('click', () => setTheme(root.dataset.theme === 'dark' ? 'light' : 'dark'));

  /* ---------- 中文 / English：按元素的英文原文查表替换 ---------- */
  // 词条是 [英文原文的开头, 中文]：短的要完全相同，24 个字符以上的按开头匹配，取最长的那条
  const LIST = window.FIBO_ZH || [], EXACT = new Map(LIST), used = window.FIBO_ZH_USED = new Set();
  const find = key => {
    if (EXACT.has(key)) { used.add(key); return EXACT.get(key); }
    let best = null;
    for (const [k, v] of LIST) if (k.length >= 24 && key.startsWith(k) && (!best || k.length > best[0].length)) best = [k, v];
    if (best) used.add(best[0]);
    return best ? best[1] : undefined;
  };
  const INLINE = new Set(['EM', 'B', 'I', 'CODE', 'BR', 'KBD', 'SPAN', 'STRONG', 'SMALL', 'U']);
  const norm = s => s.replace(/\s+/g, ' ').trim();
  const langBtn = $('#langbtn'), title = document.title;
  function setLang(lang) {
    root.lang = lang === 'zh' ? 'zh-CN' : 'en';
    save('fibo-landing-lang', lang);
    $$('h1,h2,h3,h4,p,a,button,summary,li,small,span,output').forEach(el => {
      if (!el.isConnected || el.closest('svg,[data-noi18n]')) return;
      const key = el.dataset.en !== undefined ? el.dataset.en : norm(el.textContent);
      const cn = find(key);
      if (cn === undefined) return;
      const kids = [...el.children];
      if (kids.every(c => INLINE.has(c.tagName))) {
        if (el.dataset.en === undefined) { el.dataset.en = key; el._html = el.innerHTML; }
        el.innerHTML = lang === 'zh' ? cn : el._html;
      } else {
        // 里面有图标、按钮之类：只换那一段文字
        const tn = [...el.childNodes].filter(n => n.nodeType === 3 && n.nodeValue.trim());
        if (tn.length !== 1) return;
        if (el.dataset.en === undefined) { el.dataset.en = key; el._text = tn[0].nodeValue; }
        tn[0].nodeValue = lang === 'zh' ? cn.replace(/<[^>]+>/g, '') : el._text;
      }
    });
    document.title = lang === 'zh' ? 'Fibo：在 Mac 上同时跑很多个 Coding Agent' : title;
    if (langBtn) { langBtn.textContent = lang === 'zh' ? 'EN' : '中文'; langBtn.setAttribute('aria-label', lang === 'zh' ? 'Switch to English' : '切换到中文'); }
    redraw.forEach(f => f());
  }
  if (langBtn) langBtn.addEventListener('click', () => setLang(zh() ? 'en' : 'zh'));
  if (load('fibo-landing-lang') === 'zh') setLang('zh');
})();
