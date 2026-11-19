/**
 * FLICK // INTERACTIVE HARDWARE COMPANION SCRIPT
 * Manages physical 3-way rocker switch demonstration,
 * telemetry table switching, and download placeholder handler.
 */

document.addEventListener('DOMContentLoaded', () => {
  initRockerSwitch();
  initDownloadPlaceholders();
  initSmoothScroll();
});

/**
 * Precision Smooth Scroll for Internal Navigation
 */
function initSmoothScroll() {
  document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
      const targetId = this.getAttribute('href');
      if (!targetId || targetId === '#') return;

      const targetEl = document.querySelector(targetId);
      if (targetEl) {
        e.preventDefault();
        const headerOffset = 76;
        const elementPosition = targetEl.getBoundingClientRect().top;
        const offsetPosition = elementPosition + window.pageYOffset - headerOffset;

        window.scrollTo({
          top: offsetPosition,
          behavior: 'smooth'
        });
      }
    });
  });
}

/**
 * 3-Way Rocker Switch Clipboard Engine Telemetry
 */
function initRockerSwitch() {
  const rockerKeys = document.querySelectorAll('.rocker-key');
  const modeNameEl = document.getElementById('display-mode-name');
  const modeSpecEl = document.getElementById('display-spec');
  const tbodyEl = document.getElementById('behavior-tbody');

  if (!rockerKeys.length || !tbodyEl) return;

  const modeData = {
    multi: {
      title: 'MODE: 01 SMART_MULTI (RECOMMENDED)',
      spec: 'INJECTS: CF_HDROP + CF_UNICODETEXT + CF_DIB',
      rows: [
        { app: 'File Explorer', fmt: 'CF_HDROP', desc: 'Pastes actual file copy into directory' },
        { app: 'Cursor / Antigravity / Claude', fmt: 'CF_HDROP / CF_DIB', desc: 'Attaches image directly into AI promptbox' },
        { app: 'Windows Terminal / PowerShell', fmt: 'CF_UNICODETEXT', desc: 'Pastes full absolute local path string' },
        { app: 'Figma / MS Paint / Photoshop', fmt: 'CF_DIB', desc: 'Pastes raw uncompressed 24-bit bitmap onto canvas' },
        { app: 'WhatsApp Web / Discord / Chrome', fmt: 'CF_HDROP / CF_DIB', desc: 'Uploads image directly into message composer' }
      ]
    },
    path: {
      title: 'MODE: 02 FORCE_PATH',
      spec: 'INJECTS: CF_UNICODETEXT ONLY',
      rows: [
        { app: 'Windows Terminal / PowerShell', fmt: 'CF_UNICODETEXT', desc: 'Pastes local path string directly for command line execution' },
        { app: 'Cursor / Antigravity / Claude', fmt: 'CF_UNICODETEXT', desc: 'Pastes path string (e.g. C:\\Downloads\\file.jpg) without attaching binary' },
        { app: 'VS Code Editor (Code Tab)', fmt: 'CF_UNICODETEXT', desc: 'Pastes file path string directly into source file' },
        { app: 'Notion / Word / Docs', fmt: 'CF_UNICODETEXT', desc: 'Pastes text path string instead of uploading image file' },
        { app: 'File Explorer', fmt: 'CF_UNICODETEXT', desc: 'Pastes path text into address bar or rename box' }
      ]
    },
    bitmap: {
      title: 'MODE: 03 FORCE_BITMAP',
      spec: 'INJECTS: CF_DIB BITMAP (EXIF-ROTATED 24-BIT BMP)',
      rows: [
        { app: 'Figma Desktop / Browser', fmt: 'CF_DIB', desc: 'Pastes instant pixel-perfect image layer onto canvas' },
        { app: 'MS Paint / Photoshop', fmt: 'CF_DIB', desc: 'Pastes raw bitmap data directly into active document' },
        { app: 'PowerPoint / Word / OneNote', fmt: 'CF_DIB', desc: 'Embeds graphic frame directly from memory buffer' },
        { app: 'Discord / Slack / Telegram', fmt: 'CF_DIB', desc: 'Pastes raster bitmap directly into message box' },
        { app: 'File Explorer', fmt: 'N/A', desc: 'Ignored by Explorer filesystem (use 01 SMART_MULTI for file drops)' }
      ]
    }
  };

  rockerKeys.forEach(btn => {
    btn.addEventListener('click', () => {
      const modeKey = btn.getAttribute('data-mode');
      if (!modeData[modeKey]) return;

      // Update active state on buttons
      rockerKeys.forEach(k => {
        k.classList.remove('active');
        k.setAttribute('aria-selected', 'false');
      });
      btn.classList.add('active');
      btn.setAttribute('aria-selected', 'true');

      // Update telemetry display
      const data = modeData[modeKey];
      if (modeNameEl) modeNameEl.textContent = data.title;
      if (modeSpecEl) modeSpecEl.textContent = data.spec;

      // Rebuild table rows
      tbodyEl.innerHTML = data.rows.map(row => `
        <tr>
          <td><code>${escapeHtml(row.app)}</code></td>
          <td><span class="fmt-badge">${escapeHtml(row.fmt)}</span></td>
          <td>${escapeHtml(row.desc)}</td>
        </tr>
      `).join('');
    });
  });
}

/**
 * Handle Download Placeholder Clicks
 */
function initDownloadPlaceholders() {
  const downloadLinks = document.querySelectorAll('a[id^="download-"], a[id^="card-download-"]');

  downloadLinks.forEach(link => {
    link.addEventListener('click', (e) => {
      const href = link.getAttribute('href');
      // If still set to placeholder '#'
      if (href === '#' || href === '') {
        e.preventDefault();
        showToast('Download link placeholder: Replace href="#" in index.html with your GitHub Release or Cloudflare R2 binary URL.');
      }
    });
  });
}

/**
 * Minimal Industrial Hardware Toast Notification
 */
let toastTimeout = null;
function showToast(message) {
  let toast = document.getElementById('flick-toast');
  if (!toast) {
    toast = document.createElement('div');
    toast.id = 'flick-toast';
    toast.style.cssText = `
      position: fixed;
      bottom: 24px;
      left: 50%;
      transform: translateX(-50%) translateY(20px);
      background-color: #17191E;
      color: #EDEDED;
      border: 1px solid #FF4400;
      border-radius: 6px;
      padding: 12px 20px;
      font-family: 'Space Mono', monospace;
      font-size: 0.78rem;
      line-height: 1.4;
      box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6);
      z-index: 9999;
      opacity: 0;
      transition: transform 180ms ease, opacity 180ms ease;
      max-width: 90vw;
      text-align: center;
      pointer-events: none;
    `;
    document.body.appendChild(toast);
  }

  toast.textContent = message;

  requestAnimationFrame(() => {
    toast.style.opacity = '1';
    toast.style.transform = 'translateX(-50%) translateY(0)';
  });

  if (toastTimeout) clearTimeout(toastTimeout);
  toastTimeout = setTimeout(() => {
    toast.style.opacity = '0';
    toast.style.transform = 'translateX(-50%) translateY(20px)';
  }, 4000);
}

function escapeHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
