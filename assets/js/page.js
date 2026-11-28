// Runs on the static article and archive pages (news/*.html). Those are written in English at deploy
// time so each story keeps one crawlable URL; this fills in the navigation and, when the visitor's
// language has a content overlay, swaps the article text for the translated one. Crawlers, which don't
// run this, keep getting the English page. See assets/i18n/README.md.
import { initI18n, t, locale, loadContent } from './i18n.js';

const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

function setText(el, text) {
  if (el && text) el.textContent = text;
}

// Category badges and times appear on cards as well as on the story itself.
function localizeMeta() {
  for (const el of $$('[data-cat]')) el.textContent = t(`cat.${el.dataset.cat}`);
  for (const el of $$('time[datetime]')) {
    const d = new Date(el.getAttribute('datetime'));
    if (Number.isNaN(d.getTime())) continue;
    el.textContent = `${d.toLocaleString(locale(), { dateStyle: 'long', timeStyle: 'short', timeZone: 'UTC' })} UTC`;
  }
}

function localizeCard(card, n) {
  setText(card.querySelector('h3'), n.title);
  setText(card.querySelector('.story-body p'), n.dek);
}

async function main() {
  // Sets the language, translates the chrome (nav, footer, section labels) and mounts the picker.
  await initI18n();
  const content = await loadContent();
  if (!content) return; // English: the page as written is already right.

  localizeMeta();

  const id = document.body.dataset.story;
  const own = id ? content.news?.[id] : null;
  if (own) {
    if (own.title) {
      document.title = `${own.title} · PortWatch VI`;
      setText(document.querySelector('.reader-inner h1'), own.title);
    }
    setText(document.querySelector('.reader-inner .dek'), own.dek);
    // Rebuild rather than overwrite in place: an overlay may carry more or fewer paragraphs than the
    // English page, and mixing the two would leave half a story in the wrong language.
    const box = document.querySelector('.article');
    if (box && own.body?.length) {
      box.replaceChildren(...own.body.map((text) => {
        const p = document.createElement('p');
        p.textContent = text;
        return p;
      }));
    }
  } else if (!id) {
    document.title = `${t('news.archiveTitle')} · PortWatch VI`;
  }

  for (const card of $$('[data-news-id]')) {
    const n = content.news?.[card.dataset.newsId];
    if (n) localizeCard(card, n);
  }
}

main();
