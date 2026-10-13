// La vitrine : reconnaît l'appareil pour mettre en avant la bonne façon
// d'installer Alley, et propose le bouton « Installer » là où le navigateur
// le permet (Chrome, Edge, Android, Windows, ChromeOS, Linux).

function detectOs() {
  const ua = navigator.userAgent;
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  // Les iPad récents se présentent comme des Mac : on les reconnaît au tactile.
  if (/iPhone|iPad|iPod/.test(ua) || (/Mac/.test(platform) && navigator.maxTouchPoints > 1)) return "ios";
  if (/Android/.test(ua)) return "android";
  if (/CrOS/.test(ua)) return "chromeos";
  if (/Win/.test(platform)) return "windows";
  if (/Mac/.test(platform)) return "macos";
  if (/Linux/.test(platform)) return "linux";
  return null;
}

const os = detectOs();
const card = os && document.querySelector(`.platform[data-os="${os}"]`);
if (card) {
  card.classList.add("current");
  card.parentElement.prepend(card);
}

const standalone = matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
const hint = document.getElementById("install-hint");
const button = document.getElementById("install");

if (standalone) {
  hint.textContent = "Alley est déjà installé sur cet appareil ✨";
} else if (os === "ios") {
  hint.textContent = "Sur iPhone et iPad : ouvrez cette page dans Safari, touchez Partager ⎋ puis « Sur l'écran d'accueil ».";
}

let deferred = null;
addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  deferred = event;
  button.hidden = false;
  hint.textContent = "Un clic, et Alley s'ouvre dans sa propre fenêtre.";
});
button.addEventListener("click", async () => {
  if (!deferred) return;
  deferred.prompt();
  await deferred.userChoice.catch(() => null);
  deferred = null;
  button.hidden = true;
});
addEventListener("appinstalled", () => {
  hint.textContent = "Alley est installé ✨ Retrouvez-le avec vos autres applications.";
});

if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js").catch(() => {});
