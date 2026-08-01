const pageLinks = document.querySelectorAll('[data-page]');
const sections = document.querySelectorAll('.page-section');
const menuLinks = document.querySelectorAll('.menu [data-page]');
const tabs = document.querySelectorAll('.tab');
const panes = document.querySelectorAll('.form-pane');

function showPage(name) {
  sections.forEach(section => section.classList.toggle('active', section.id === name));
  menuLinks.forEach(link => link.classList.toggle('active', link.dataset.page === name));
  window.scrollTo(0, 0);
}

function activateTab(name) {
  tabs.forEach(tab => tab.classList.toggle('active', tab.dataset.tab === name));
  panes.forEach(pane => pane.classList.toggle('active', pane.dataset.pane === name));
}

pageLinks.forEach(element => element.addEventListener('click', event => {
  const page = element.dataset.page;
  if (!page) return;
  event.preventDefault();
  showPage(page);
  if (element.dataset.tab) activateTab(element.dataset.tab);
  history.replaceState(null, '', `#${page}`);
}));

tabs.forEach(tab => tab.addEventListener('click', () => activateTab(tab.dataset.tab)));

const hash = location.hash.replace('#', '');
if (['home', 'about', 'auth', 'dashboard'].includes(hash)) showPage(hash);
document.documentElement.dataset.agilang = 'ready';
