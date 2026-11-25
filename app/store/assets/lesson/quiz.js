// Quiz widget.
//
// Markup:
//   <div class="quiz" data-answer="2" data-why="Explanation shown after any answer.">
//     <p class="q">Question?</p>
//     <ol><li>Option 1</li><li>Option 2</li><li>Option 3</li></ol>
//   </div>
//   <p class="score"></p>   (optional, shows the running score)
//
// data-answer is the 1-based index of the right option.

(function () {
  function init() {
    const quizzes = Array.from(document.querySelectorAll(".quiz"));
    const scoreEl = document.querySelector(".score");
    let answered = 0;
    let correct = 0;

    function updateScore() {
      if (!scoreEl) return;
      scoreEl.textContent =
        "Score: " + correct + " / " + quizzes.length +
        (answered === quizzes.length ? "  (done)" : "");
    }

    quizzes.forEach(function (quiz) {
      const answer = parseInt(quiz.dataset.answer, 10);
      const options = Array.from(quiz.querySelectorAll("li"));
      const fb = document.createElement("p");
      fb.className = "fb";
      quiz.appendChild(fb);

      let firstTry = true;

      options.forEach(function (li, i) {
        li.setAttribute("role", "button");
        li.setAttribute("tabindex", "0");

        function choose() {
          const isRight = i + 1 === answer;
          li.classList.add(isRight ? "right" : "wrong");
          fb.className = "fb " + (isRight ? "right" : "wrong");
          fb.textContent = (isRight ? "Right. " : "Not quite. Try again. ") +
            (isRight ? (quiz.dataset.why || "") : "");

          if (firstTry) {
            answered += 1;
            if (isRight) correct += 1;
            firstTry = false;
            updateScore();
          }
        }

        li.addEventListener("click", choose);
        li.addEventListener("keydown", function (e) {
          if (e.key === "Enter" || e.key === " ") { e.preventDefault(); choose(); }
        });
      });
    });

    updateScore();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
