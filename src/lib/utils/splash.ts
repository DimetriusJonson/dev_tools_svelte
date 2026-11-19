export const SPLASH_ELEMENT_ID = 'app-splash';
export const SPLASH_TITLE_ELEMENT_ID = 'app-splash-title';
export const SPLASH_VERSION_ELEMENT_ID = 'app-splash-version';

/** Shortest time the splash stays on screen so a fast hydration does not flash. */
export const MIN_SPLASH_VISIBLE_MS = 350;

const SPLASH_FADE_MS = 260;

/**
 * Writes the localized app title into the splash. `src/app.html` guesses it from the
 * stored locale before the bundle exists; the layout overwrites it with `app.title`.
 */
export function setSplashTitle(title: string): void {
	const element = document.getElementById(SPLASH_TITLE_ELEMENT_ID);
	if (!element) return;
	element.textContent = title;
}

/** Writes the app version into the splash; a no-op once the splash is gone. */
export function setSplashVersion(version: string): void {
	const element = document.getElementById(SPLASH_VERSION_ELEMENT_ID);
	if (!element) return;
	element.textContent = version;
	element.hidden = false;
}

/** Remaining time the splash should still be visible after `elapsed` ms of boot. */
export function splashHoldMs(elapsed: number, minimumMs = MIN_SPLASH_VISIBLE_MS): number {
	return Math.max(0, minimumMs - elapsed);
}

/**
 * Fades out and removes the pre-hydration splash defined in `src/app.html`.
 * `elapsed` defaults to `performance.now()`, i.e. the time since navigation start.
 */
export function dismissSplash(
	options: { minimumMs?: number; fadeMs?: number; elapsed?: number } = {}
): void {
	const splash = document.getElementById(SPLASH_ELEMENT_ID);
	if (!splash) return;
	const fadeMs = options.fadeMs ?? SPLASH_FADE_MS;
	const elapsed = options.elapsed ?? performance.now();
	const hold = splashHoldMs(elapsed, options.minimumMs ?? MIN_SPLASH_VISIBLE_MS);
	window.setTimeout(() => {
		splash.classList.add('app-splash-out');
		window.setTimeout(() => splash.remove(), fadeMs);
	}, hold);
}
