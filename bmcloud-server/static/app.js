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
const jobsPanel = document.getElementById('jobs-panel');
const jobsTable = document.getElementById('jobs');
const jobsBody = document.getElementById('jobs-body');
const jobsMsg = document.getElementById('jobs-msg');
const flashDeviceSelect = document.getElementById('flash-device');
const firmwareFileInput = document.getElementById('firmware-file');
const firmwareFileName = document.getElementById('firmware-file-name');
const flashBtn = document.getElementById('flash-btn');

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
let devicesById = new Map(); // last known devices, to resolve names in the jobs table

async function poll() {
    try {
        await load();
        await loadJobs();
    } catch (e) {
        msg.textContent = 'Server unreachable';
    }
    pollTimer = setTimeout(poll, 5000); // schedule the next poll only after this one finishes
}

function enter() {
    passwordInput.value = '';
    authSection.hidden = true;
    panelSection.hidden = false;
    jobsPanel.hidden = false;
    pollTimer = setTimeout(poll, 0)
}

async function doLogout() {
    await api('/logout', 'POST');
    authSection.hidden = false;
    panelSection.hidden = true;
    jobsPanel.hidden = true;
    table.hidden = true;
    tbody.replaceChildren();
    jobsTable.hidden = true;
    jobsBody.replaceChildren();
    newTokenBox.hidden = true;
    msg.textContent = '';
    jobsMsg.textContent = '';
    firmwareFileInput.value = '';
    renderFileName();

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
        jobsPanel.hidden = true;
        authError('Session expired, log in again');
        return;
    }
    const devices = await resp.json();
    devicesById = new Map(devices.map((d) => [d.id, d]));
    renderDeviceSelect(devices);
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
            await loadJobs();
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

async function loadJobs() {
    const resp = await api('/jobs', 'GET');
    if (resp.status === 401) {
        authSection.hidden = false;
        panelSection.hidden = true;
        jobsPanel.hidden = true;
        authError('Session expired, log in again');
        return;
    }
    const jobs = await resp.json();
    jobsMsg.textContent = jobs.length === 0 ? 'No jobs yet' : '';
    jobsTable.hidden = false;
    jobsBody.replaceChildren();
    for (const j of jobs) {
        const tr = document.createElement('tr');

        const idTd = document.createElement('td');
        idTd.textContent = j.job_id;
        tr.appendChild(idTd);

        const deviceTd = document.createElement('td');
        deviceTd.textContent = devicesById.get(j.device_id)?.name ?? '#' + j.device_id;
        tr.appendChild(deviceTd);

        const fwTd = document.createElement('td');
        fwTd.textContent = j.fw_name;
        tr.appendChild(fwTd);

        const statusTd = document.createElement('td');
        statusTd.textContent = j.status;
        statusTd.className = j.status;
        tr.appendChild(statusTd);

        const errTd = document.createElement('td');
        errTd.textContent = j.error ?? '';
        tr.appendChild(errTd);

        jobsBody.appendChild(tr);
    }
}

// Rebuilt on every devices refresh; keeps the current selection if the device still exists.
function renderDeviceSelect(devices) {
    const prev = flashDeviceSelect.value;
    flashDeviceSelect.replaceChildren();
    for (const d of devices) {
        const opt = document.createElement('option');
        opt.value = d.id;
        opt.textContent = d.name;
        flashDeviceSelect.appendChild(opt);
    }
    if (prev !== '' && devices.some((d) => String(d.id) === prev)) {
        flashDeviceSelect.value = prev;
    }
}

// The flash endpoint takes the raw file as the body and its name in the query -
// the shared api() helper only speaks JSON, so this one is a plain fetch.
async function createJob() {
    const deviceId = flashDeviceSelect.value;
    if (deviceId === '') {
        jobsMsg.textContent = 'No device to flash - create one first';
        return;
    }
    const file = firmwareFileInput.files[0];
    if (!file) {
        jobsMsg.textContent = 'Choose a firmware file';
        return;
    }
    flashBtn.disabled = true;
    try {
        const resp = await fetch('/device/' + deviceId + '/flash?name=' + encodeURIComponent(file.name), {
            method: 'POST',
            headers: { 'Content-Type': 'application/octet-stream' },
            body: file
        });
        if (resp.status === 201) {
            jobsMsg.textContent = '';
            firmwareFileInput.value = '';
            renderFileName();
            await loadJobs();
            return;
        }
        const err = await resp.json().catch(() => null);
        jobsMsg.textContent = err ? err.error : 'Failed to create job';
    } catch (e) {
        jobsMsg.textContent = 'Server unreachable';
    } finally {
        flashBtn.disabled = false;
    }
}

function formatSize(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

function renderFileName() {
    const file = firmwareFileInput.files[0];
    firmwareFileName.textContent = file ? file.name + ' · ' + formatSize(file.size) : 'no file chosen';
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
flashBtn.addEventListener('click', createJob);
firmwareFileInput.addEventListener('change', renderFileName);
renderFileName();
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
