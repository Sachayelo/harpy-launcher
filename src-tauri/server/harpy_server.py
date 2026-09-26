"""Controls the Harpy game servers. The launcher's workshop sends this file to
the server over SSH on every call and runs it as root:

    ssh root@host python3 - <action> [server] [--force]

Actions: status, start, stop, restart, deploy, rollback. For `deploy`, the
launcher puts `MANIFEST_B64 = "..."` in front of this file: the pack manifest
it fetched and whose signature it already checked.

Only the mods folder is ever changed. Server settings, worlds and mods that
are not part of the pack (spark...) stay untouched; mods reserved to the
client are not installed. Every change is preceded by a backup of the mods
folder, which `rollback` puts back.

Progress goes to stdout line by line; the result is a final `HARPY_JSON:` line
and failures a `HARPY_ERROR:` line with a non-zero exit code.
"""

import base64
import hashlib
import json
import os
import re
import shutil
import socket
import ssl
import struct
import sys
import time
import urllib.request
import zipfile
from datetime import datetime, timezone

SERVERS = {
    'prod': {'uuid': '44fedf99-b0bd-43f6-ab97-c25c4923d5ca', 'port': 25565, 'channel': 'prod'},
    'dev': {'uuid': '88f28ee1-610c-4bef-8766-827ba9c05ccd', 'port': 25566, 'channel': 'dev'},
}
VOLUMES = '/var/lib/pelican/volumes'
HOME = '/root/harpy'
KEEP_BACKUPS = 5

try:
    MANIFEST_B64
except NameError:
    MANIFEST_B64 = None


class Failure(Exception):
    pass


def say(text):
    print(text, flush=True)


# --- Wings, Pelican's daemon on this machine --------------------------------

def wings_token():
    with open('/etc/pelican/config.yml', encoding='utf-8') as config:
        for line in config:
            match = re.match(r'^token:\s*"?([^"\s]+)"?', line)
            if match:
                return match.group(1)
    raise Failure('Jeton de Wings introuvable.')


def wings(method, path, body=None):
    request = urllib.request.Request(
        f'https://127.0.0.1:8443/api/servers/{path}',
        data=json.dumps(body).encode() if body is not None else None,
        method=method,
        headers={
            'Authorization': f'Bearer {wings_token()}',
            'Accept': 'application/json',
            'Content-Type': 'application/json',
        },
    )
    # Local call: the daemon's certificate is issued for its public name.
    context = ssl._create_unverified_context()
    with urllib.request.urlopen(request, context=context, timeout=20) as response:
        data = response.read()
    return json.loads(data) if data else None


def details(server):
    return wings('GET', SERVERS[server]['uuid'])


def state(server):
    return details(server).get('state', 'unknown')


def players(server):
    """(online, max) from the server list ping, None when it doesn't answer."""
    port = SERVERS[server]['port']

    def varint(value):
        out = b''
        while True:
            byte = value & 0x7F
            value >>= 7
            out += bytes([byte | (0x80 if value else 0)])
            if not value:
                return out

    def read_varint(sock):
        result, shift = 0, 0
        while True:
            chunk = sock.recv(1)
            if not chunk:
                raise OSError('connection closed')
            result |= (chunk[0] & 0x7F) << shift
            if not chunk[0] & 0x80:
                return result
            shift += 7

    try:
        with socket.create_connection(('127.0.0.1', port), timeout=4) as sock:
            host = b'127.0.0.1'
            handshake = varint(0) + varint(767) + varint(len(host)) + host + struct.pack('>H', port) + varint(1)
            sock.sendall(varint(len(handshake)) + handshake + varint(1) + varint(0))
            read_varint(sock)
            read_varint(sock)
            length = read_varint(sock)
            data = b''
            while len(data) < length:
                chunk = sock.recv(length - len(data))
                if not chunk:
                    raise OSError('connection closed')
                data += chunk
        status = json.loads(data)
        return status['players']['online'], status['players']['max']
    except (OSError, ValueError, KeyError):
        return None


def wait_until(condition, timeout, step=2):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if condition():
            return True
        time.sleep(step)
    return False


def guard_players(server, force):
    online = players(server)
    if online and online[0] > 0 and not force:
        raise Failure(f'{online[0]} joueur(s) connecté(s) sur ce serveur.')


def stop(server):
    """Stops the server if it runs. True when it was running."""
    if state(server) == 'offline':
        return False
    say('Arrêt du serveur (sauvegarde du monde)…')
    wings('POST', f"{SERVERS[server]['uuid']}/power", {'action': 'stop'})
    if not wait_until(lambda: state(server) == 'offline', 120):
        say('Le serveur ne s\'arrête pas : arrêt forcé.')
        wings('POST', f"{SERVERS[server]['uuid']}/power", {'action': 'kill'})
        if not wait_until(lambda: state(server) == 'offline', 30):
            raise Failure('Le serveur refuse de s\'arrêter.')
    say('Serveur arrêté.')
    return True


def start(server):
    say('Démarrage du serveur…')
    wings('POST', f"{SERVERS[server]['uuid']}/power", {'action': 'start'})
    if not wait_until(lambda: players(server) is not None, 300, step=3):
        raise Failure('Le serveur ne répond toujours pas après 5 minutes : regarde sa console dans le panel.')
    say('Serveur en ligne.')


# --- Mods ------------------------------------------------------------------

def mods_dir(server):
    return f"{VOLUMES}/{SERVERS[server]['uuid']}/mods"


def mod_info(path):
    """(mod id, environment) read from the jar's fabric.mod.json."""
    try:
        with zipfile.ZipFile(path) as jar:
            text = jar.read('fabric.mod.json').decode('utf-8', 'replace')
    except (KeyError, zipfile.BadZipFile, OSError):
        return None, '*'
    try:
        data = json.loads(text, strict=False)
        return data.get('id'), data.get('environment', '*')
    except ValueError:
        match = re.search(r'"id"\s*:\s*"([^"]+)"', text)
        return (match.group(1) if match else None), '*'


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as file:
        for block in iter(lambda: file.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def download(url, destination, expected_sha, expected_size):
    request = urllib.request.Request(url, headers={'User-Agent': 'HarpyServer'})
    with urllib.request.urlopen(request, timeout=120) as response, open(destination, 'wb') as out:
        shutil.copyfileobj(response, out)
    if os.path.getsize(destination) != expected_size or sha256(destination) != expected_sha:
        os.remove(destination)
        raise Failure(f'{os.path.basename(destination)} ne correspond pas au pack (téléchargement abîmé ?).')


def state_path(server):
    return f'{HOME}/state/{server}.json'


def load_state(server):
    try:
        with open(state_path(server), encoding='utf-8') as file:
            return json.load(file)
    except (OSError, ValueError):
        return {}


def save_state(server, data):
    os.makedirs(f'{HOME}/state', exist_ok=True)
    with open(state_path(server) + '.tmp', 'w', encoding='utf-8') as file:
        json.dump(data, file, indent=2)
    os.replace(state_path(server) + '.tmp', state_path(server))


def backups_dir(server):
    return f'{HOME}/backups/{server}'


def backups(server):
    try:
        return sorted(os.listdir(backups_dir(server)))
    except OSError:
        return []


def backup(server):
    stamp = datetime.now().strftime('%Y-%m-%d_%H-%M-%S')
    target = f'{backups_dir(server)}/{stamp}'
    shutil.copytree(mods_dir(server), f'{target}/mods', symlinks=True)
    if os.path.exists(state_path(server)):
        shutil.copy2(state_path(server), f'{target}/state.json')
    for old in backups(server)[:-KEEP_BACKUPS]:
        shutil.rmtree(f'{backups_dir(server)}/{old}', ignore_errors=True)
    say(f'Sauvegarde des mods actuels : {stamp}')


def give_to_pelican(server, path):
    owner = os.stat(mods_dir(server))
    os.chown(path, owner.st_uid, owner.st_gid)


# --- Actions ---------------------------------------------------------------

def action_status(_server, _force):
    result = {}
    for key in SERVERS:
        info = {'state': 'unknown', 'online': None, 'max': None, 'memoryMb': None, 'memoryLimitMb': None}
        try:
            data = details(key)
            info['state'] = data.get('state', 'unknown')
            usage = data.get('utilization') or {}
            if usage.get('memory_bytes'):
                info['memoryMb'] = usage['memory_bytes'] // (1024 * 1024)
            if usage.get('memory_limit_bytes'):
                info['memoryLimitMb'] = usage['memory_limit_bytes'] // (1024 * 1024)
        except Exception:
            pass
        if info['state'] == 'running':
            online = players(key)
            if online:
                info['online'], info['max'] = online
        saved = load_state(key)
        info.update({
            'version': saved.get('version'),
            'deployedAt': saved.get('deployedAt'),
            'backups': len(backups(key)),
        })
        result[key] = info
    return result


def action_start(server, _force):
    if state(server) != 'offline':
        raise Failure('Le serveur est déjà allumé.')
    start(server)


def action_stop(server, force):
    guard_players(server, force)
    if not stop(server):
        raise Failure('Le serveur est déjà arrêté.')


def action_restart(server, force):
    guard_players(server, force)
    stop(server)
    start(server)


def action_deploy(server, force):
    if not MANIFEST_B64:
        raise Failure('Manifeste du pack manquant.')
    manifest = json.loads(base64.b64decode(MANIFEST_B64))
    if manifest.get('channel') != SERVERS[server]['channel']:
        raise Failure('Ce manifeste n\'est pas celui de ce serveur.')
    directory = mods_dir(server)
    pack = [f for f in manifest['files'] if f['path'].startswith('mods/') and f['path'].endswith('.jar')]
    pack_ids = {f['modId'] for f in pack if f.get('modId')}
    say(f"Pack {manifest['version']} ({manifest['channel']}) : {len(pack)} mods")

    on_server = {}
    for name in sorted(os.listdir(directory)):
        if name.endswith('.jar'):
            mod_id, _ = mod_info(f'{directory}/{name}')
            on_server.setdefault(mod_id or name, []).append(name)

    staging = f'{HOME}/staging/{server}'
    shutil.rmtree(staging, ignore_errors=True)
    os.makedirs(staging)
    add, remove = [], []
    for file in pack:
        name = os.path.basename(file['path'])
        existing = on_server.get(file.get('modId') or name, [])
        if existing == [name] and sha256(f'{directory}/{name}') == file['sha256']:
            continue
        download(file['url'], f'{staging}/{name}', file['sha256'], file['size'])
        if not existing and mod_info(f'{staging}/{name}')[1] == 'client':
            say(f'  {name} : réservé au client, pas installé sur le serveur')
            os.remove(f'{staging}/{name}')
            continue
        add.append(name)
        remove.extend(existing)

    # Mods installed by an earlier deployment that have left the pack.
    for name, mod_id in load_state(server).get('files', {}).items():
        if mod_id not in pack_ids and name not in add and os.path.exists(f'{directory}/{name}'):
            remove.append(name)

    if not add and not remove:
        say('Les mods du serveur sont déjà ceux du pack.')
    else:
        for name in sorted(set(remove) - set(add)):
            say(f'  - {name}')
        for name in add:
            say(f'  + {name}')
        guard_players(server, force)
        was_running = stop(server)
        backup(server)
        for name in set(remove):
            if os.path.exists(f'{directory}/{name}'):
                os.remove(f'{directory}/{name}')
        for name in add:
            shutil.move(f'{staging}/{name}', f'{directory}/{name}')
            give_to_pelican(server, f'{directory}/{name}')
        say(f'{len(add)} mod(s) installé(s), {len(set(remove) - set(add))} retiré(s).')
        if was_running:
            start(server)
        else:
            say('Le serveur était arrêté : il le reste.')
    shutil.rmtree(staging, ignore_errors=True)

    deployed = {}
    for file in pack:
        name = os.path.basename(file['path'])
        if os.path.exists(f'{directory}/{name}'):
            deployed[name] = file.get('modId') or name
    save_state(server, {
        'channel': manifest['channel'],
        'version': manifest['version'],
        'deployedAt': datetime.now(timezone.utc).isoformat(timespec='seconds'),
        'files': deployed,
    })


def action_rollback(server, force):
    available = backups(server)
    if not available:
        raise Failure('Aucune sauvegarde à restaurer.')
    latest = f'{backups_dir(server)}/{available[-1]}'
    say(f'Retour aux mods du {available[-1]}')
    guard_players(server, force)
    was_running = stop(server)
    directory = mods_dir(server)
    for name in os.listdir(directory):
        if name.endswith('.jar'):
            os.remove(f'{directory}/{name}')
    for name in os.listdir(f'{latest}/mods'):
        if name.endswith('.jar'):
            shutil.copy2(f'{latest}/mods/{name}', f'{directory}/{name}')
            give_to_pelican(server, f'{directory}/{name}')
    if os.path.exists(f'{latest}/state.json'):
        shutil.copy2(f'{latest}/state.json', state_path(server))
    elif os.path.exists(state_path(server)):
        os.remove(state_path(server))
    shutil.rmtree(latest)
    say('Mods restaurés.')
    if was_running:
        start(server)


ACTIONS = {
    'status': action_status,
    'start': action_start,
    'stop': action_stop,
    'restart': action_restart,
    'deploy': action_deploy,
    'rollback': action_rollback,
}


def main(args):
    action = args[0] if args else ''
    server = args[1] if len(args) > 1 else None
    force = '--force' in args
    if action not in ACTIONS or (action != 'status' and server not in SERVERS):
        raise Failure('Commande inconnue.')
    result = ACTIONS[action](server, force)
    print('HARPY_JSON:' + json.dumps(result), flush=True)


if __name__ == '__main__':
    try:
        main(sys.argv[1:])
    except Failure as failure:
        print(f'HARPY_ERROR:{failure}', flush=True)
        sys.exit(1)
    except Exception as error:
        print(f'HARPY_ERROR:{type(error).__name__}: {error}', flush=True)
        sys.exit(1)
