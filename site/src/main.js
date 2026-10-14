const root = document.documentElement;
root.classList.add("js");

const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)");

document.addEventListener("DOMContentLoaded", () => {
  navMaterial();
  copyButtons();
  videos();
  productStage();
  // Every plugin or none: a blocker that drops one file would otherwise throw halfway
  // through setup and leave the pipeline unlit for good.
  const { gsap, ScrollTrigger, SplitText, CustomEase } = window;
  if (!reduceMotion.matches && gsap && ScrollTrigger && SplitText && CustomEase) motion();
});

function navMaterial() {
  const nav = document.querySelector(".nav");
  const update = () => nav.classList.toggle("is-scrolled", scrollY > 8);
  addEventListener("scroll", update, { passive: true });
  update();
}

function copyButtons() {
  for (const button of document.querySelectorAll("[data-copy]")) {
    const target = document.querySelector(button.dataset.copy);
    const idle = button.textContent;
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(target.textContent.trim());
        button.textContent = button.dataset.done;
      } catch {
        // Clipboard can be denied (permission, insecure context); leave the command
        // selected so ⌘C still works, and say so on the button.
        getSelection().selectAllChildren(target);
        button.textContent = button.dataset.fail;
      }
      setTimeout(() => (button.textContent = idle), 1800);
    });
  }
}

function videos() {
  if (reduceMotion.matches) return;
  const loops = document.querySelectorAll("video[data-autoplay]");
  const io = new IntersectionObserver((entries) => {
    for (const { target, isIntersecting } of entries) {
      if (isIntersecting) target.play().catch(() => {});
      else target.pause();
    }
  }, { threshold: 0.25 });
  loops.forEach((v) => io.observe(v));

  // The celebration plays while the CTA is hovered or focused. On leave it finishes the
  // current cycle instead of cutting: the clip ends on the pose it starts from.
  for (const video of document.querySelectorAll("video[data-trigger]")) {
    const trigger = document.querySelector(video.dataset.trigger);
    const start = () => { video.loop = true; if (video.paused) { video.currentTime = 0; video.play().catch(() => {}); } };
    const stop = () => { video.loop = false; };
    trigger.addEventListener("pointerenter", start);
    trigger.addEventListener("focus", start);
    trigger.addEventListener("pointerleave", stop);
    trigger.addEventListener("blur", stop);
  }
}

function productStage() {
  const desktop = matchMedia("(min-width: 900px)");
  const screens = [...document.querySelectorAll(".product__screen")];
  const steps = [...document.querySelectorAll(".step")];
  const dots = [...document.querySelectorAll(".product__dots button")];
  if (!screens.length) return;

  const show = (i) => {
    for (const el of [...screens, ...steps, ...dots]) el.classList.toggle("is-active", Number(el.dataset.step) === i);
    // Only the visible screenshot is announced; the others sit at opacity 0.
    for (const img of screens) img.setAttribute("aria-hidden", String(Number(img.dataset.step) !== i));
  };
  // The step whose middle crosses the viewport's middle drives the stage. Works with plain
  // scrolling; no pin is needed because the stage is position: sticky.
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting && desktop.matches) show(Number(e.target.dataset.step));
  }, { rootMargin: "-50% 0px -50% 0px" });
  steps.forEach((s) => io.observe(s));

  dots.forEach((dot) => dot.addEventListener("click", () => {
    steps[Number(dot.dataset.step)].scrollIntoView({ behavior: reduceMotion.matches ? "auto" : "smooth", block: "center" });
  }));
  show(0);
}

function motion() {
  const { gsap, ScrollTrigger, SplitText, CustomEase } = window;
  gsap.registerPlugin(ScrollTrigger, SplitText, CustomEase);
  CustomEase.create("snap", "0.32,0.72,0,1");

  for (const el of document.querySelectorAll("[data-reveal]")) {
    SplitText.create(el, {
      type: "lines", mask: "lines", autoSplit: true, aria: "none",
      onSplit: (self) => gsap.from(self.lines, {
        yPercent: 100, duration: 0.8, ease: "snap", stagger: 0.08,
        scrollTrigger: { trigger: el, start: "top 88%", once: true },
      }),
    });
  }

  for (const el of document.querySelectorAll("[data-count]")) {
    const end = Number(el.dataset.count);
    if (end === 0) continue;
    const state = { v: 0 };
    gsap.to(state, {
      v: end, duration: 1, ease: "snap",
      onUpdate: () => (el.textContent = String(Math.round(state.v))),
      scrollTrigger: { trigger: el, start: "top 90%", once: true },
    });
  }

  gsap.from(".bars span", {
    scaleY: 0, ease: "none", stagger: 0.04,
    scrollTrigger: { trigger: ".bars", start: "top 85%", end: "top 45%", scrub: true },
  });

  // The rail fills with scroll progress and lights each stage as the fill reaches it.
  // Stacked (phones) there is no rail: per-stage connectors light with the stages.
  const pipeline = document.querySelector(".pipeline");
  const fill = pipeline.querySelector(".pipeline__rail span");
  const cards = [...pipeline.querySelectorAll(".stage-card")];
  const stacked = matchMedia("(max-width: 860px)");
  const track = (progress) => {
    gsap.set(fill, { scaleX: progress });
    // In a row each stage lights when the rail reaches its icon; stacked, the last one
    // would only light once the whole list had scrolled past, so the steps come earlier.
    const steps = stacked.matches ? cards.length : cards.length - 1;
    cards.forEach((card, i) => card.classList.toggle("is-lit", progress >= i / steps - 0.001));
  };
  pipeline.classList.add("is-tracking");
  ScrollTrigger.create({
    trigger: pipeline, start: "top 80%", end: "bottom 55%", scrub: true,
    onUpdate: (self) => track(self.progress),
    onRefresh: (self) => track(self.progress),
  });
}
