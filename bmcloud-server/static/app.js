const authSection = document.getElementById('auth');
const panelSection = document.getElementById('panel');
const emailInput = document.getElementById('email');
const passwordInput = document.getElementById('password');
const authMsg = document.getElementById('auth-msg');
const msg = document.getElementById('msg');
const table = document.getElementById('devices');
const tbody = document.getElementById('devices-body');
const newDeviceInput = document.getElementById('new-device-name');
const addDeviceBtn = document.getElementById('add-device-btn');
const newTokenBox = document.getElementById('new-token');
const newTokenName = document.getElementById('new-token-name');
const newTokenValue = document.getElementById('new-token-value');
const copyTokenBtn = document.getElementById('copy-token-btn');
const copyMsg = document.getElementById('copy-msg');

async function api(path, method, body) {
    const headers = {};
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    return fetch(path, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body)
    });
}

function authError(text) {
    authMsg.textContent = text;
    authMsg.className = 'err';
}

function clearAuthMsg() {
    authMsg.textContent = '';
    authMsg.className = '';
}

async function doRegister() {
    clearAuthMsg();
    const resp = await api('/register', 'POST', {
        email: emailInput.value.trim(),
        password: passwordInput.value
    });
    if (resp.status === 201) {
        await doLogin();
        return;
    }
    const err = await resp.json().catch(() => null);
    authError(err ? err.error : 'Registration failed');
}

async function doLogin() {
    clearAuthMsg();
    const resp = await api('/login', 'POST', {
        email: emailInput.value.trim(),
        password: passwordInput.value
    }, false);
    if (resp.status !== 200) {
        const err = await resp.json().catch(() => null);
        authError(err ? err.error : 'Log in failed');
        return;
    }
    enter();
}

let pollTimer = null;
let editing = null; // id of the device whose name is currently being renamed

async function poll() {
    try {
        await load();
    } catch (e) {
        msg.textContent = 'Server unreachable';
    }
    pollTimer = setTimeout(poll, 5000); // schedule the next poll only after this one finishes
}

function enter() {
    passwordInput.value = '';
    authSection.hidden = true;
    panelSection.hidden = false;
    pollTimer = setTimeout(poll, 0)
}

async function doLogout() {
    await api('/logout', 'POST');
    authSection.hidden = false;
    panelSection.hidden = true;
    table.hidden = true;
    tbody.replaceChildren();
    newTokenBox.hidden = true;
    msg.textContent = '';

    clearTimeout(pollTimer);
    pollTimer = null;
}

function timeAgo(unixSec) {
    const s = Math.floor(Date.now() / 1000) - unixSec;
    if (s < 60) return 'just now';
    const m = Math.floor(s / 60);
    if (m < 60) return m + ' min ago';
    const h = Math.floor(m / 60);
    if (h < 24) return h + ' h ago';
    return Math.floor(h / 24) + ' days ago';
}

async function load() {
    if (editing !== null) return; // do not re-render the device table while renaming
    const resp = await api('/devices', 'GET');
    if (resp.status === 401) {
        authSection.hidden = false;
        panelSection.hidden = true;
        authError('Session expired, log in again');
        return;
    }
    const devices = await resp.json();
    msg.textContent = devices.length === 0 ? 'No devices yet' : '';
    table.hidden = false;
    tbody.replaceChildren();
    for (const d of devices) {
        const tr = document.createElement('tr');
        tr.className = d.state;

        const idTd = document.createElement('td');
        idTd.textContent = d.id;
        tr.appendChild(idTd);

        const nameTd = document.createElement('td');
        nameTd.appendChild(makeNameSpan(d));
        tr.appendChild(nameTd);

        for (const value of [d.state, timeAgo(d.updated_at)]) {
            const td = document.createElement('td');
            td.textContent = value;
            tr.appendChild(td);
        }
        const actions = document.createElement('td');
        const del = document.createElement('button');
        del.textContent = '✕';
        del.title = 'delete device';
        del.className = 'device-del';
        del.addEventListener('click', () => deleteDevice(d.id, del));
        actions.appendChild(del);
        tr.appendChild(actions);
        tbody.appendChild(tr);
    }
}

function makeNameSpan(d) {
    const span = document.createElement('span');
    span.className = 'device-name';
    span.textContent = d.name;
    span.title = 'click to rename';
    span.addEventListener('click', () => startRename(d.id, d.name, span));
    return span;
}

function startRename(id, oldName, span) {
    if (editing !== null) return; // another name is already being edited
    editing = id;

    const input = document.createElement('input');
    input.className = 'device-name-input';
    input.value = oldName;
    span.replaceWith(input);
    input.focus();
    input.select();

    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            e.preventDefault();
            input.blur(); // save
        } else if (e.key === 'Escape') {
            input.value = oldName; // blur will see the old name and skip the request
            input.blur();
        }
    });
    input.addEventListener('blur', () => finishRename(id, oldName, input));
}

function finishRename(id, oldName, input) {
    editing = null;
    const newName = input.value.trim();
    // Empty or unchanged name - just restore the text, no request needed.
    if (newName === '' || newName === oldName) {
        input.replaceWith(makeNameSpan({ id, name: oldName }));
        return;
    }
    // Optimistically show the new name; the request is sent in parallel.
    input.replaceWith(makeNameSpan({ id, name: newName }));
    api('/device/' + id, 'PATCH', { name: newName })
        .then((resp) => {
            msg.textContent = resp.status === 204 ? '' : 'Failed to rename device ' + id;
        })
        .catch(() => {
            msg.textContent = 'Server unreachable';
        });
}

async function deleteDevice(id, btn) {
    btn.disabled = true;
    try {
        const resp = await api('/device/' + id, 'DELETE');
        if (resp.status === 204) {
            msg.textContent = '';
            await load();
            return;
        }
        msg.textContent = 'Failed to delete device ' + id;
    } catch (e) {
        msg.textContent = 'Server unreachable';
    } finally {
        btn.disabled = false;
    }
}

async function createDevice() {
    const name = newDeviceInput.value.trim();
    if (name === '') {
        msg.textContent = 'Enter a device name';
        newDeviceInput.focus();
        return;
    }
    addDeviceBtn.disabled = true;
    try {
        const resp = await api('/devices', 'POST', { name });
        if (resp.status === 201) {
            const body = await resp.json(); // { device, token } - the token is never shown again
            msg.textContent = '';
            newDeviceInput.value = '';
            newTokenName.textContent = body.device.name;
            newTokenValue.textContent = body.token;
            newTokenBox.hidden = false;
            await load();
            return;
        }
        const err = await resp.json().catch(() => null);
        msg.textContent = err ? err.error : 'Failed to create device';
    } catch (e) {
        msg.textContent = 'Server unreachable';
    } finally {
        addDeviceBtn.disabled = false;
    }
}

let copyTimer = null;

async function copyToken() {
    const value = newTokenValue.textContent;
    let ok = false;
    try {
        await navigator.clipboard.writeText(value);
        ok = true;
    } catch (e) {
        // clipboard API needs a secure context (https / localhost) - fall back for plain http
        const ta = document.createElement('textarea');
        ta.value = value;
        document.body.appendChild(ta);
        ta.select();
        ok = document.execCommand('copy');
        ta.remove();
    }
    copyMsg.textContent = ok ? 'copied!' : 'copy failed - select it manually';
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copyMsg.textContent = ''; }, 2000);
}

document.getElementById('login-btn').addEventListener('click', doLogin);
document.getElementById('register-btn').addEventListener('click', doRegister);
document.getElementById('logout-btn').addEventListener('click', doLogout);
addDeviceBtn.addEventListener('click', createDevice);
copyTokenBtn.addEventListener('click', copyToken);
newDeviceInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        e.preventDefault();
        createDevice();
    }
});

async function init() {
    const resp = await api('/devices', 'GET');
    if (resp.status === 200) enter();
}
init();
