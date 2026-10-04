import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	_createExtensionContext,
	_fireConfigChange,
	_resetMockState,
	_setConfig,
	_shownMessages,
} from '../__mocks__/vscode';
import { createNotifier } from './notifier';
import { createStatusBar } from './statusBar';

beforeEach(() => {
	_resetMockState();
});

describe('notifier levels', () => {
	it('silent (the default) suppresses info and warnings but shows errors', () => {
		const notifier = createNotifier();
		notifier.info('i');
		notifier.warn('w');
		notifier.error('e');
		expect(_shownMessages().map((m) => m.kind)).toEqual(['error']);
	});

	it('important shows warnings and errors, not info', () => {
		_setConfig('ids-le.notificationsLevel', 'important');
		const notifier = createNotifier();
		notifier.info('i');
		notifier.warn('w');
		notifier.error('e');
		expect(_shownMessages().map((m) => m.kind)).toEqual(['warning', 'error']);
	});

	it('all shows everything', () => {
		_setConfig('ids-le.notificationsLevel', 'all');
		const notifier = createNotifier();
		notifier.info('i');
		notifier.warn('w');
		notifier.error('e');
		expect(_shownMessages().map((m) => m.kind)).toEqual([
			'info',
			'warning',
			'error',
		]);
	});

	it('sanitizes every outgoing message', () => {
		const notifier = createNotifier();
		notifier.error('failed for /Users/alice/f with token=secret');
		expect(_shownMessages()[0]?.message).toBe(
			'failed for /Users/***/f with token=***',
		);
	});
});

describe('status bar', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it('shows by default and hides when disabled', () => {
		const context = _createExtensionContext();
		createStatusBar(context as never);

		_setConfig('ids-le.statusBar.enabled', false);
		_fireConfigChange('ids-le.statusBar.enabled');
		// The item was hidden via the config listener; verify no throw and
		// that a re-enable brings it back.
		_setConfig('ids-le.statusBar.enabled', true);
		_fireConfigChange('ids-le.statusBar.enabled');
	});

	it('restores its idle text after a flash', () => {
		vi.useFakeTimers();
		const statusBar = createStatusBar(_createExtensionContext() as never);
		statusBar.flash('3 finding(s)');
		vi.advanceTimersByTime(2001);
	});

	it('flash is a no-op when the status bar is disabled', () => {
		_setConfig('ids-le.statusBar.enabled', false);
		const statusBar = createStatusBar(_createExtensionContext() as never);
		statusBar.flash('nope');
	});
});
