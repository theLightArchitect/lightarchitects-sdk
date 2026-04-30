import type { Page } from '@playwright/test';
import { expect } from '@playwright/test';

/**
 * Page Object — top navigation bar.
 *
 * Encapsulates all nav interactions so tests don't hard-code selectors.
 * All navigation methods await the target route to settle before returning.
 */
export class NavPage {
  // Nav buttons — role-based, resilient to text changes only via this class
  readonly activityBtn  = this.page.getByRole('button', { name: 'Activity',  exact: true });
  readonly sitrepBtn    = this.page.getByRole('button', { name: 'Sitrep',    exact: true });
  readonly queueBtn     = this.page.getByRole('button', { name: 'Queue',     exact: true });
  readonly intakeBtn    = this.page.getByRole('button', { name: 'Intake',    exact: true });
  readonly squadBtn     = this.page.getByRole('button', { name: 'Squad',     exact: true });
  readonly memoryToggle = this.page.getByTestId('memory-toggle');
  readonly helixToggle  = this.page.getByTestId('helix-toggle');

  constructor(private readonly page: Page) {}

  async goToActivity(): Promise<void> {
    await this.activityBtn.click();
    await this.page.waitForURL('**#/activity**', { timeout: 5_000 });
  }

  async goToQueue(): Promise<void> {
    await this.queueBtn.click();
    await this.page.waitForURL(/\/#\/?$/, { timeout: 5_000 });
  }

  async goToIntake(): Promise<void> {
    await this.intakeBtn.click();
    await this.page.waitForURL('**#/intake**', { timeout: 5_000 });
  }

  async goToSitrep(): Promise<void> {
    await this.sitrepBtn.click();
    await this.page.waitForURL('**#/sitrep**', { timeout: 5_000 });
  }

  async goToSquadDispatch(): Promise<void> {
    await this.squadBtn.click();
    await this.page.waitForURL('**#/squad-dispatch**', { timeout: 5_000 });
  }

  /** Cmd/Ctrl+K global shortcut → Squad Dispatch */
  async pressDispatchShortcut(): Promise<void> {
    await this.page.keyboard.press('Meta+k');
    await this.page.waitForURL('**#/squad-dispatch**', { timeout: 5_000 });
  }

  async assertNavLabels(): Promise<void> {
    await expect.soft(this.activityBtn).toBeVisible();
    await expect.soft(this.sitrepBtn).toBeVisible();
    await expect.soft(this.queueBtn).toBeVisible();
    await expect.soft(this.intakeBtn).toBeVisible();
    await expect.soft(this.squadBtn).toBeVisible();
  }
}
