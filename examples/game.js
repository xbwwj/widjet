/// <reference path="../widjet.d.ts" />

// A small Touhou-style bullet-hell prototype.
// Run with `cargo run --release -- user2.js` for smooth 60 FPS bullet motion.
//   * portrait canvas with a black play field,
//   * red enemy triangle firing red circular bullets,
//   * white player triangle controlled with the pointer,
//   * score strip at the bottom,
//   * collision ends the run; click / press to restart.

const canvas = new WidgetCanvas();

const WIDTH = 540;
const HEIGHT = 900;
const STRIP_HEIGHT = 56;
const PLAY_HEIGHT = HEIGHT - STRIP_HEIGHT;

// Put the game near the top of the screen, centered horizontally.
canvas.anchor = WidgetAnchor.TOP;
canvas.marginTop = 30;
canvas.width = WIDTH;
canvas.height = HEIGHT;

const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));

const PLAYER_RADIUS = 9;
const PLAYER_SIZE = 20;
const ENEMY_SIZE = 26;
const BULLET_RADIUS = 6;
const MAX_BULLETS = 260;

let gameOver = false;
let score = 0;
let elapsedSeconds = 0;
let lastTimestamp = 0;
let spawnTimer = 0;
let ringTimer = 3.0;
let invulnerable = 1.0;
let exited = false;
let pointerX = WIDTH / 2;
let pointerY = PLAY_HEIGHT / 2;

const EXIT_BUTTON = {
  x: WIDTH / 2 - 72,
  y: PLAY_HEIGHT / 2 + 104,
  w: 144,
  h: 46,
};

const player = {
  x: WIDTH / 2,
  y: PLAY_HEIGHT - 70,
  targetX: WIDTH / 2,
  targetY: PLAY_HEIGHT - 70,
};

const enemy = {
  x: WIDTH / 2,
  y: 90,
};

/** @type {{x: number, y: number, vx: number, vy: number, r: number}[]} */
let bullets = [];

/**
 * @param {number} value
 * @param {number} min
 * @param {number} max
 */
function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

/**
 * @param {number} x
 * @param {number} y
 * @param {{x: number, y: number, w: number, h: number}} rect
 */
function pointInRect(x, y, rect) {
  return (
    x >= rect.x && x <= rect.x + rect.w && y >= rect.y && y <= rect.y + rect.h
  );
}

function resetGame() {
  gameOver = false;
  score = 0;
  elapsedSeconds = 0;
  lastTimestamp = 0;
  spawnTimer = 0;
  ringTimer = 3.0;
  invulnerable = 1.0;
  bullets = [];
  player.x = WIDTH / 2;
  player.y = PLAY_HEIGHT - 70;
  player.targetX = player.x;
  player.targetY = player.y;
}

/** @param {number} seconds */
function spawnAimedFan(seconds) {
  const difficulty = Math.min(1, seconds / 60);
  const count = 3 + Math.floor(difficulty * 4);
  const spread = 0.16 + difficulty * 0.12;
  const speed = 170 + difficulty * 150;
  const baseAngle = Math.atan2(player.y - enemy.y, player.x - enemy.x);

  for (let i = 0; i < count; i += 1) {
    const offset = (i - (count - 1) / 2) * spread;
    const angle = baseAngle + offset;
    bullets.push({
      x: enemy.x,
      y: enemy.y + 14,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      r: BULLET_RADIUS,
    });
  }
}

/** @param {number} seconds */
function spawnRing(seconds) {
  const count = 14;
  const speed = 120 + Math.min(1, seconds / 60) * 100;
  for (let i = 0; i < count; i += 1) {
    const angle = (i / count) * Math.PI * 2 + seconds * 0.3;
    bullets.push({
      x: enemy.x,
      y: enemy.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      r: BULLET_RADIUS,
    });
  }
}

/** @param {number} dt */
function updateBullets(dt) {
  const next = [];
  for (const bullet of bullets) {
    bullet.x += bullet.vx * dt;
    bullet.y += bullet.vy * dt;

    if (
      bullet.x < -20 ||
      bullet.x > WIDTH + 20 ||
      bullet.y < -20 ||
      bullet.y > PLAY_HEIGHT + 20
    ) {
      continue;
    }

    const dx = bullet.x - player.x;
    const dy = bullet.y - player.y;
    const collisionRadius = bullet.r + PLAYER_RADIUS;
    if (
      !gameOver &&
      invulnerable <= 0 &&
      dx * dx + dy * dy <= collisionRadius * collisionRadius
    ) {
      gameOver = true;
      console.log(
        `game over: score=${Math.floor(score)} time=${elapsedSeconds.toFixed(1)}s`,
      );
    }

    next.push(bullet);
  }
  bullets = next;

  if (bullets.length > MAX_BULLETS) {
    bullets.splice(0, bullets.length - MAX_BULLETS);
  }
}

function drawBackground() {
  ctx.fillStyle = "#000000";
  ctx.fillRect(0, 0, WIDTH, HEIGHT);

  // A very subtle star field made from a few fixed dots. Batch every dot
  // into one path so the renderer performs a single fill command.
  ctx.fillStyle = "#161616";
  ctx.beginPath();
  for (let i = 0; i < 24; i += 1) {
    const x = (i * 97 + 31) % WIDTH;
    const y = (i * 173 + 53) % PLAY_HEIGHT;
    ctx.rect(x, y, 2, 2);
  }
  ctx.fill();
}

function drawBullets() {
  if (bullets.length === 0) return;

  ctx.fillStyle = "#ff334f";
  ctx.beginPath();
  for (const bullet of bullets) {
    ctx.moveTo(bullet.x + bullet.r, bullet.y);
    ctx.arc(bullet.x, bullet.y, bullet.r, 0, Math.PI * 2);
  }
  ctx.fill();
}

/** @param {number} seconds */
function drawEnemy(seconds) {
  const wobble = Math.sin(seconds * 3.2) * 3;
  const size = ENEMY_SIZE + wobble;

  ctx.fillStyle = "#ff3048";
  ctx.beginPath();
  ctx.moveTo(enemy.x - size / 2, enemy.y - size / 2);
  ctx.lineTo(enemy.x + size / 2, enemy.y - size / 2);
  ctx.lineTo(enemy.x, enemy.y + size / 2);
  ctx.closePath();
  ctx.fill();
}

function drawPlayer() {
  ctx.fillStyle = "#ffffff";
  ctx.beginPath();
  ctx.moveTo(player.x, player.y - PLAYER_SIZE / 2);
  ctx.lineTo(player.x + PLAYER_SIZE / 2, player.y + PLAYER_SIZE / 2);
  ctx.lineTo(player.x - PLAYER_SIZE / 2, player.y + PLAYER_SIZE / 2);
  ctx.closePath();
  ctx.fill();
}

function drawScoreStrip() {
  ctx.fillStyle = "#080808";
  ctx.fillRect(0, PLAY_HEIGHT, WIDTH, STRIP_HEIGHT);

  ctx.fillStyle = "#ff334f";
  ctx.fillRect(0, PLAY_HEIGHT, WIDTH, 3);

  ctx.fillStyle = "#ffffff";
  ctx.font = "900 20px monospace";
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  ctx.fillText(
    `SCORE ${String(Math.floor(score)).padStart(7, "0")}`,
    18,
    PLAY_HEIGHT + STRIP_HEIGHT / 2 + 1,
  );

  ctx.textAlign = "right";
  ctx.fillText(
    `${elapsedSeconds.toFixed(1)}s`,
    WIDTH - 18,
    PLAY_HEIGHT + STRIP_HEIGHT / 2 + 1,
  );

  // Also show score progress visually, useful even while the font is loading.
  ctx.fillStyle = "#1c0a0e";
  ctx.fillRect(18, PLAY_HEIGHT + STRIP_HEIGHT - 8, WIDTH - 36, 3);
  ctx.fillStyle = "#ff334f";
  const progress = (Math.floor(score) % 10000) / 10000;
  ctx.fillRect(18, PLAY_HEIGHT + STRIP_HEIGHT - 8, (WIDTH - 36) * progress, 3);
}

function drawGameOver() {
  ctx.fillStyle = "rgba(0, 0, 0, 0.72)";
  ctx.fillRect(0, 0, WIDTH, PLAY_HEIGHT);

  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  ctx.fillStyle = "#ff334f";
  ctx.font = "900 52px sans-serif";
  ctx.fillText("GAME OVER", WIDTH / 2, PLAY_HEIGHT / 2 - 26);

  ctx.fillStyle = "#ffffff";
  ctx.font = "900 24px monospace";
  ctx.fillText(
    `SCORE ${String(Math.floor(score)).padStart(7, "0")}`,
    WIDTH / 2,
    PLAY_HEIGHT / 2 + 36,
  );

  ctx.fillStyle = "#9a9a9a";
  ctx.font = "700 16px sans-serif";
  ctx.fillText("click / press to restart", WIDTH / 2, PLAY_HEIGHT / 2 + 70);

  // Exit button.
  const button = EXIT_BUTTON;
  const hovered = pointInRect(pointerX, pointerY, button);

  ctx.fillStyle = hovered ? "#421019" : "#1a0a0d";
  ctx.fillRect(button.x, button.y, button.w, button.h);

  ctx.strokeStyle = hovered ? "#ff667d" : "#ff334f";
  ctx.lineWidth = 2;
  ctx.strokeRect(button.x, button.y, button.w, button.h);

  // Draw a small X icon so the button is recognizable even without fonts.
  const iconX = button.x + 26;
  const iconY = button.y + button.h / 2;
  ctx.strokeStyle = "#ff334f";
  ctx.lineWidth = 3;
  ctx.beginPath();
  ctx.moveTo(iconX - 7, iconY - 7);
  ctx.lineTo(iconX + 7, iconY + 7);
  ctx.moveTo(iconX + 7, iconY - 7);
  ctx.lineTo(iconX - 7, iconY + 7);
  ctx.stroke();

  ctx.fillStyle = "#ffffff";
  ctx.font = "900 20px monospace";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(
    "EXIT",
    button.x + button.w / 2 + 12,
    button.y + button.h / 2 + 1,
  );
}

/** @param {number} timestamp */
function frame(timestamp) {
  if (exited) return;

  const dt =
    lastTimestamp === 0
      ? 1 / 60
      : Math.min((timestamp - lastTimestamp) / 1000, 0.05);
  lastTimestamp = timestamp;

  if (!gameOver) {
    elapsedSeconds += dt;
    score += dt * 100;
    invulnerable = Math.max(0, invulnerable - dt);

    enemy.x = WIDTH / 2 + Math.sin(elapsedSeconds * 1.25) * (WIDTH * 0.31);
    enemy.y = 90 + Math.sin(elapsedSeconds * 2.1) * 18;

    // Smoothly follow the pointer so the player has a little inertia.
    const follow = Math.min(1, dt * 18);
    player.x += (player.targetX - player.x) * follow;
    player.y += (player.targetY - player.y) * follow;

    player.x = clamp(player.x, PLAYER_RADIUS, WIDTH - PLAYER_RADIUS);
    player.y = clamp(
      player.y,
      PLAY_HEIGHT * 0.45,
      PLAY_HEIGHT - PLAYER_RADIUS - 4,
    );

    spawnTimer -= dt;
    if (spawnTimer <= 0) {
      spawnTimer = Math.max(0.22, 0.55 - elapsedSeconds * 0.004);
      spawnAimedFan(elapsedSeconds);
    }

    ringTimer -= dt;
    if (ringTimer <= 0) {
      ringTimer = Math.max(2.0, 4.0 - elapsedSeconds * 0.02);
      spawnRing(elapsedSeconds);
    }

    updateBullets(dt);
  }

  drawBackground();
  drawBullets();
  drawEnemy(elapsedSeconds);
  drawPlayer();
  drawScoreStrip();
  if (gameOver) drawGameOver();

  requestAnimationFrame(frame);
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/** @param {WidgetEvent} event */
function trackPointer(event) {
  pointerX = event.x;
  pointerY = event.y;
}

/** @param {WidgetEvent} event */
function movePlayerTo(event) {
  if (gameOver) return;
  player.targetX = clamp(event.x, PLAYER_RADIUS, WIDTH - PLAYER_RADIUS);
  player.targetY = clamp(
    event.y,
    PLAY_HEIGHT * 0.45,
    PLAY_HEIGHT - PLAYER_RADIUS - 4,
  );
}

/** @param {WidgetEvent} event */
function handlePointerDown(event) {
  trackPointer(event);

  if (gameOver) {
    if (pointInRect(event.x, event.y, EXIT_BUTTON)) {
      exited = true;
      canvas.destroy();
      return;
    }
    resetGame();
  }

  movePlayerTo(event);
}

canvas.addEventListener("pointermove", (event) => {
  trackPointer(event);
  movePlayerTo(event);
});
canvas.addEventListener("pointerdown", handlePointerDown);

canvas.onresize = () => {
  // The game uses a fixed logical resolution; nothing to recompute.
};

requestAnimationFrame(frame);
