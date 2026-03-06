use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(inline_js = r#"
export function highlight_markdown_code() {
  const hljs = globalThis.hljs;
  if (!hljs) {
    setTimeout(highlight_markdown_code, 120);
    return;
  }

  function getLanguageFromClass(codeEl) {
    for (const cls of codeEl.classList) {
      if (cls.startsWith('language-')) return cls.slice('language-'.length).toLowerCase();
      if (cls.startsWith('lang-')) return cls.slice('lang-'.length).toLowerCase();
    }
    return '';
  }

  function normalizeLanguage(raw) {
    const lang = (raw ?? '').toLowerCase().trim();
    if (lang === 'py') return 'python';
    if (lang === 'js') return 'javascript';
    if (lang === 'ts') return 'typescript';
    if (lang === 'sh' || lang === 'shell' || lang === 'zsh') return 'bash';
    if (lang === 'yml') return 'yaml';
    if (lang === 'md') return 'markdown';
    return lang;
  }

  requestAnimationFrame(() => {
    document.querySelectorAll('.preview pre').forEach((pre) => {
      const code = pre.querySelector('code');
      if (!code) return;

      const lang = normalizeLanguage(getLanguageFromClass(code));
      const source = code.textContent ?? '';

      if (!lang) {
        pre.removeAttribute('data-language');
        code.textContent = source;
        code.classList.remove('hljs');
        return;
      }

      pre.setAttribute('data-language', lang);
      if (!hljs.getLanguage(lang)) {
        code.textContent = source;
        code.classList.add('hljs');
        return;
      }

      const highlighted = hljs.highlight(source, { language: lang, ignoreIllegals: true });
      code.innerHTML = highlighted.value;
      code.classList.add('hljs');
    });
  });
}

export function auto_resize_editors() {
  requestAnimationFrame(() => {
    document.querySelectorAll('.editor').forEach((el) => {
      if (!(el instanceof HTMLTextAreaElement)) return;
      el.style.height = 'auto';
      el.style.overflowY = 'hidden';
      el.style.height = `${el.scrollHeight}px`;
    });
  });
}

export function init_sidebar_resizer() {
  const app = document.querySelector('.app');
  if (!app || app.dataset.sidebarResizeInit === '1') return;
  app.dataset.sidebarResizeInit = '1';

  const handle = app.querySelector('.sidebar-resizer');
  if (!handle) return;

  const minWidth = 220;
  const maxWidth = 520;
  const prefKey = 'slate.ui.sidebar_width';

  try {
    const stored = globalThis.localStorage?.getItem(prefKey);
    if (stored) {
      const parsed = Number.parseInt(stored, 10);
      if (!Number.isNaN(parsed)) {
        const clamped = Math.max(minWidth, Math.min(maxWidth, parsed));
        app.style.setProperty('--sidebar-width', `${clamped}px`);
      }
    }
  } catch {}

  const onPointerMove = (event) => {
    const raw = event.clientX;
    const clamped = Math.max(minWidth, Math.min(maxWidth, raw));
    app.style.setProperty('--sidebar-width', `${clamped}px`);
    try {
      globalThis.localStorage?.setItem(prefKey, String(clamped));
    } catch {}
  };

  const onPointerUp = () => {
    document.body.style.userSelect = '';
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
  };

  handle.addEventListener('pointerdown', (event) => {
    if (app.getAttribute('data-sidebar') === 'collapsed') return;
    event.preventDefault();
    document.body.style.userSelect = 'none';
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
  });
}

export function ui_pref_get(key) {
  try {
    return globalThis.localStorage?.getItem(`slate.ui.${key}`) ?? '';
  } catch {
    return '';
  }
}

export function ui_pref_set(key, value) {
  try {
    globalThis.localStorage?.setItem(`slate.ui.${key}`, value);
  } catch {}
}

export function ui_prompt(message, defaultValue = '') {
  const value = globalThis.prompt(message, defaultValue);
  return value ?? '';
}

export function click_by_id(id) {
  const node = document.getElementById(id);
  if (node instanceof HTMLInputElement) {
    node.click();
  }
}

export function media_create_object_url(bytes, mimeType) {
  const typed = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const blob = new Blob([typed], { type: mimeType || 'application/octet-stream' });
  return URL.createObjectURL(blob);
}

export function media_revoke_object_url(url) {
  if (url) {
    URL.revokeObjectURL(url);
  }
}

export function download_data_url(filename, dataUrl) {
  if (!dataUrl) return;
  const anchor = document.createElement('a');
  anchor.href = dataUrl;
  anchor.download = filename || 'download';
  anchor.rel = 'noopener';
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

export function file_to_data_url(file) {
  return new Promise((resolve, reject) => {
    if (!(file instanceof File)) {
      reject(new Error('Expected File'));
      return;
    }
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error || new Error('Failed to read file'));
    reader.readAsDataURL(file);
  });
}
"#)]
extern "C" {
    pub fn highlight_markdown_code();
    pub fn auto_resize_editors();
    pub fn init_sidebar_resizer();
    pub fn ui_pref_get(key: &str) -> String;
    pub fn ui_pref_set(key: &str, value: &str);
    pub fn ui_prompt(message: &str, default_value: &str) -> String;
    pub fn click_by_id(id: &str);
    pub fn media_create_object_url(bytes: &[u8], mime_type: &str) -> String;
    pub fn media_revoke_object_url(url: &str);
    pub fn download_data_url(filename: &str, data_url: &str);
    pub fn file_to_data_url(file: web_sys::File) -> js_sys::Promise;
}
