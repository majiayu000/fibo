// Puts this plugin's status line into Claude Code's settings (`setup`) and takes it out
// again (`teardown`). The status line set before is kept in the plugin's data directory
// and keeps printing: statusline.sh hands it the same input. Run by Mooring with
// MOORING_PLUGIN_DIR and MOORING_PLUGIN_DATA; JXA, so it needs nothing beyond macOS.
ObjC.import('Foundation');

const env = $.NSProcessInfo.processInfo.environment;
const getenv = (k) => {
  const v = env.objectForKey(k);
  return v.isNil() ? '' : v.js;
};
const fm = $.NSFileManager.defaultManager;
const exists = (p) => fm.fileExistsAtPath(p);
const readText = (p) => {
  const s = $.NSString.stringWithContentsOfFileEncodingError(p, $.NSUTF8StringEncoding, null);
  if (s.isNil()) throw new Error('读不了 ' + p);
  return s.js;
};
const writeText = (p, text) => {
  if (!$(text).writeToFileAtomicallyEncodingError(p, true, $.NSUTF8StringEncoding, null)) {
    throw new Error('写不了 ' + p);
  }
};
// A copy of the settings can hold the keys they hold, whatever the original's permissions.
const BACKUP_MODE = 0o600;
const ownerOnly = (p) => fm.setAttributesOfItemAtPathError($({ NSFilePosixPermissions: BACKUP_MODE }), p, null);
const remove = (p) => {
  if (exists(p)) fm.removeItemAtPathError(p, null);
};
const quote = (s) => "'" + s.split("'").join("'\\''") + "'";

const home = getenv('HOME');
const dir = getenv('MOORING_PLUGIN_DIR');
const data = getenv('MOORING_PLUGIN_DATA');
if (!home || !dir || !data) throw new Error('要由 Mooring 运行：缺少 HOME、MOORING_PLUGIN_DIR 或 MOORING_PLUGIN_DATA');
const backup = data + '/settings.before.json';
const config = getenv('CLAUDE_CONFIG_DIR') || home + '/.claude';
// A settings file linked from a dotfiles repository is written where it lives.
const settingsPath = $(config + '/settings.json').stringByResolvingSymlinksInPath.js;
const MARK = 'MOORING_PLUGIN=claude-quota ';
const ours = (line) => !!line && typeof line.command === 'string' && line.command.startsWith(MARK);
// The wrapper Keepline installed before the rename to Mooring, and where it kept the line it wrapped.
const keepline = (line) => !!line && typeof line.command === 'string' && line.command.includes('keepline-claude-statusline');
const keeplineOriginal = home + '/.keepline/claude-statusline-original.json';

function readSettings() {
  if (!exists(settingsPath)) return {};
  const text = readText(settingsPath);
  if (!text.trim()) return {};
  const v = JSON.parse(text);
  if (v === null || typeof v !== 'object' || Array.isArray(v)) throw new Error(settingsPath + ' 不是 JSON 对象，没有改');
  return v;
}
function writeSettings(settings) {
  fm.createDirectoryAtPathWithIntermediateDirectoriesAttributesError($(settingsPath).stringByDeletingLastPathComponent, true, $(), null);
  writeText(settingsPath, JSON.stringify(settings, null, 2) + '\n');
}
function saveOriginal(line) {
  writeText(data + '/original.json', JSON.stringify(line === undefined ? null : line) + '\n');
  if (line && line.type === 'command' && typeof line.command === 'string' && line.command.trim()) {
    writeText(data + '/original.sh', line.command + '\n');
  } else {
    remove(data + '/original.sh');
  }
}

function setup() {
  // A backup from before it was kept private; nothing else would ever rewrite it.
  if (exists(backup)) ownerOnly(backup);
  const settings = readSettings();
  const current = settings.statusLine;
  const command = MARK + 'sh ' + quote(dir + '/statusline.sh') + ' ' + quote(data);
  let original;
  if (ours(current)) {
    // Already wrapped; only the paths may have moved.
    if (current.command === command) return '已经接上，没有改动';
    settings.statusLine = Object.assign({}, current, { command });
    writeSettings(settings);
    return '更新了状态栏命令的路径';
  }
  if (keepline(current)) {
    original = exists(keeplineOriginal) ? JSON.parse(readText(keeplineOriginal)) : null;
  } else {
    original = current;
  }
  saveOriginal(original);
  if (exists(settingsPath)) {
    // Made private first: the atomic write keeps the mode of the file it replaces.
    if (!exists(backup)) fm.createFileAtPathContentsAttributes(backup, $.NSData.data, $({ NSFilePosixPermissions: BACKUP_MODE }));
    ownerOnly(backup);
    writeText(backup, readText(settingsPath));
  }
  const base = original && typeof original === 'object' ? original : {};
  settings.statusLine = Object.assign({}, base, { type: 'command', command });
  writeSettings(settings);
  return keepline(current) ? '换掉了 Keepline 的旧包装，原来的状态栏照常显示' : '接上了，原来的状态栏照常显示';
}

function teardown() {
  const settings = readSettings();
  if (!ours(settings.statusLine)) return '状态栏不是这个插件的，没有改';
  const file = data + '/original.json';
  const original = exists(file) ? JSON.parse(readText(file)) : null;
  if (original) settings.statusLine = original;
  else delete settings.statusLine;
  writeSettings(settings);
  remove(file);
  remove(data + '/original.sh');
  return original ? '还原了原来的状态栏' : '去掉了状态栏（装之前没有设）';
}

function run(argv) {
  if (argv[0] === 'setup') return setup();
  if (argv[0] === 'teardown') return teardown();
  throw new Error('用法：settings.js setup|teardown');
}
