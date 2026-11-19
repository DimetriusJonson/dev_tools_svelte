import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	dismissSplash,
	setSplashTitle,
	setSplashVersion,
	splashHoldMs,
	SPLASH_ELEMENT_ID,
	SPLASH_TITLE_ELEMENT_ID,
	SPLASH_VERSION_ELEMENT_ID
} from './splash';

function mountSplash(): HTMLElement {
	document.body.innerHTML =
		`<div id="${SPLASH_ELEMENT_ID}" class="app-splash">` +
		`<p id="${SPLASH_TITLE_ELEMENT_ID}"></p>` +
		`<p id="${SPLASH_VERSION_ELEMENT_ID}" hidden></p></div>`;
	return document.getElementById(SPLASH_ELEMENT_ID) as HTMLElement;
}

describe('splash', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
		document.body.innerHTML = '';
	});

	it('shows the app version once the app is ready', () => {
		mountSplash();
		setSplashTitle('Полезные инструменты веб-разработчика');
		setSplashVersion('0.1.0');
		expect(document.getElementById(SPLASH_TITLE_ELEMENT_ID)?.textContent).toBe(
			'Полезные инструменты веб-разработчика'
		);
		const element = document.getElementById(SPLASH_VERSION_ELEMENT_ID) as HTMLElement;
		expect(element.textContent).toBe('0.1.0');
		expect(element.hidden).toBe(false);
	});

	it('does nothing when the splash is already gone', () => {
		expect(() => {
			setSplashTitle('WebDev Useful Tools');
			setSplashVersion('0.1.0');
			dismissSplash();
		}).not.toThrow();
		expect(document.body.innerHTML).toBe('');
	});

	it('keeps the splash visible for the remaining minimum time', () => {
		expect(splashHoldMs(120, 350)).toBe(230);
		expect(splashHoldMs(350, 350)).toBe(0);
		expect(splashHoldMs(900, 350)).toBe(0);
	});

	it('fades the splash out before removing it', () => {
		const splash = mountSplash();
		dismissSplash({ minimumMs: 0, fadeMs: 200, elapsed: 1200 });
		expect(splash.classList.contains('app-splash-out')).toBe(false);
		vi.advanceTimersByTime(1);
		expect(splash.classList.contains('app-splash-out')).toBe(true);
		expect(document.body.contains(splash)).toBe(true);
		vi.advanceTimersByTime(200);
		expect(document.body.contains(splash)).toBe(false);
	});

	it('waits out the minimum visible time before fading', () => {
		const splash = mountSplash();
		dismissSplash({ minimumMs: 500, fadeMs: 100, elapsed: 0 });
		vi.advanceTimersByTime(499);
		expect(splash.classList.contains('app-splash-out')).toBe(false);
		vi.advanceTimersByTime(1);
		expect(splash.classList.contains('app-splash-out')).toBe(true);
	});
});
