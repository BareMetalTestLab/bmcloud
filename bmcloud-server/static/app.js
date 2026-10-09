const TOKEN_KEY = 'bmcloud_token';

const authSection = document.getElementById('auth');
const panelSection = document.getElementById('panel');
const loginInput = document.getElementById('login');
const emailInput = document.getElementById('email');
const passwordInput = document.getElementById('password');
const authMsg = document.getElementById('auth-msg');
const msg = document.getElementById('msg');
const table = document.getElementById('devices');
const tbody = document.getElementById('devices-body');

function token() {
    return localStorage.getItem(TOKEN_KEY) ?? '';
}

async function api(path, method, body, auth = true) {
    const headers = {};
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (auth) headers['Authorization'] = 'Bearer ' + token();
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
        login: loginInput.value.trim(),
        email: emailInput.value.trim(),
        password: passwordInput.value
    }, false);
    if (resp.status === 201) {
        await doLogin();
        return;
    }
    const err = await resp.json().catch(() => null);
    authError(err ? err.error : 'регистрация не удалась');
}

async function doLogin() {
    clearAuthMsg();
    const resp = await api('/login', 'POST', {
        email: emailInput.value.trim(),
        password: passwordInput.value
    }, false);
    if (resp.status !== 200) {
        const err = await resp.json().catch(() => null);
        authError(err ? err.error : 'не удалось войти');
        return;
    }
    const body = await resp.json();
    localStorage.setItem(TOKEN_KEY, body.token);
    enter();
}

function enter() {
    passwordInput.value = '';
    authSection.hidden = true;
    panelSection.hidden = false;
    load();
}

async function doLogout() {
    await api('/logout', 'POST');
    localStorage.removeItem(TOKEN_KEY);
    authSection.hidden = false;
    panelSection.hidden = true;
    table.hidden = true;
    tbody.replaceChildren();
    msg.textContent = '';
}

function timeAgo(unixSec) {
    const s = Math.floor(Date.now() / 1000) - unixSec;
    if (s < 60) return 'только что';
    const m = Math.floor(s / 60);
    if (m < 60) return m + ' мин назад';
    const h = Math.floor(m / 60);
    if (h < 24) return h + ' ч назад';
    return Math.floor(h / 24) + ' дн назад';
}

async function load() {
    const resp = await api('/devices', 'GET');
    if (resp.status === 401) {
        localStorage.removeItem(TOKEN_KEY);
        authSection.hidden = false;
        panelSection.hidden = true;
        authError('сессия истекла, войдите снова');
        return;
    }
    const devices = await resp.json();
    msg.textContent = devices.length === 0 ? 'Устройств пока нет' : '';
    table.hidden = false;
    tbody.replaceChildren();
    for (const d of devices) {
        const tr = document.createElement('tr');
        tr.className = d.state;
        const cells = [d.id, d.name, d.state, timeAgo(d.updated_at)];
        for (const value of cells) {
            const td = document.createElement('td');
            td.textContent = value;   // ← только так: имена вводит человек
            tr.appendChild(td);
        }
        tbody.appendChild(tr);
    }
}

document.getElementById('login-btn').addEventListener('click', doLogin);
document.getElementById('register-btn').addEventListener('click', doRegister);
document.getElementById('refresh-btn').addEventListener('click', load);
document.getElementById('logout-btn').addEventListener('click', doLogout);

// если токен уже сохранён — сразу входим
if (token()) {
    enter();
}
