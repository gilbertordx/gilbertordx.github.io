// Theme invert (black-on-white <-> white-on-black), remembered per browser.
(function () {
  var root = document.documentElement;
  var btn = document.getElementById("invert");
  if (!btn) return;
  btn.addEventListener("click", function () {
    var next = root.dataset.theme === "light" ? "dark" : "light";
    root.dataset.theme = next;
    try {
      localStorage.setItem("theme", next);
    } catch (e) {}
  });
})();
