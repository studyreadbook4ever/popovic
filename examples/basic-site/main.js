let count = 0;
const button = document.querySelector("#counter");

button?.addEventListener("click", () => {
  count += 1;
  button.textContent = `Clicked ${count} ${count === 1 ? "time" : "times"}`;
});
