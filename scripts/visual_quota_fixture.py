"""Same stale status line and HTTP snapshot, consumed by each build's shipped plugin.

MOORING_FIXTURE_CLAUDE_QUOTA=1 shows stale 10% versus sampled 19%; =error shows a
failed refresh, =reset shows an elapsed 5h window while the week remains valid.
Only private fixture data, with an unusable token and a cooldown preventing HTTP.
"""
import hashlib, json, shutil


def prepare(home, binary, now, mode):
    plugin = home / '.config/fibo/plugins/claude-quota'
    shutil.copytree(binary.parent.parent / 'plugins/claude-quota', plugin)
    data = home / '.config/fibo/plugin-data/claude-quota'
    data.mkdir(parents=True)
    def save(path, doc):
        path.write_text(json.dumps(doc, ensure_ascii=False))
    token = 'visual-fixture-not-a-credential'
    save(home / '.claude/.credentials.json', {'claudeAiOauth': {
        'accessToken': token, 'expiresAt': (now + 86400) * 1000, 'scopes': ['user:profile']}})
    save(data / 'options.json', {'usage': False})
    reset = now - 10 if mode == 'reset' else now + 4 * 3600
    save(data / 'statusline.json', {'rate_limits': {
        'five_hour': {'used_percentage': 10, 'resets_at': reset},
        'seven_day': {'used_percentage': 37, 'resets_at': now + 4 * 86400}}})
    save(data / 'http-usage.json', {
        'credential': hashlib.sha256(token.encode()).hexdigest(),
        'next_attempt': now + 600, 'retry_at': now + 600,
        'snapshot': {'at': now - 60, 'models': [],
                     'session': {'used': 19, 'resets_at': reset},
                     'week': {'used': 37, 'resets_at': now + 4 * 86400}},
        'error': '额度查询未成功：网络不可用或请求超时' if mode == 'error' else ''})
