import type { Page, Locator } from '@playwright/test';
import { expect } from '@playwright/test';

/**
 * Page Object — Squad Dispatch screen.
 *
 * Wraps all data-testid selectors and common interactions so tests stay
 * declarative and resilient to implementation changes.
 */
export class SquadDispatchPage {
  readonly taskInput:      Locator = this.page.getByTestId('dispatch-task-input');
  readonly dryToggle:      Locator = this.page.getByTestId('dispatch-dry-toggle');
  readonly submitBtn:      Locator = this.page.getByTestId('dispatch-submit');
  readonly newDispatchBtn: Locator = this.page.getByRole('button', { name: 'New Dispatch', exact: true });

  constructor(private readonly page: Page) {}

  agentBtn(agent: string): Locator {
    return this.page.getByTestId(`agent-btn-${agent.toLowerCase()}`);
  }

  agentCard(agent: string): Locator {
    return this.page.getByTestId(`agent-rail-${agent.toLowerCase()}`);
  }

  async navigate(): Promise<void> {
    await this.page.evaluate(() => { window.location.hash = '#/squad-dispatch'; });
    await this.taskInput.waitFor({ state: 'visible', timeout: 10_000 });
  }

  async fillTask(text: string): Promise<void> {
    await this.taskInput.fill(text);
  }

  async submit(): Promise<void> {
    await expect(this.submitBtn).toBeEnabled({ timeout: 5_000 });
    await this.submitBtn.click();
  }

  async waitForComplete(): Promise<void> {
    await this.page.waitForFunction(
      () => {
        const text = document.body.textContent ?? '';
        return text.includes('Done') || text.includes('New Dispatch') || /\d+\.\d+s/.test(text);
      },
      { timeout: 15_000 },
    );
  }

  async assertAgentSelected(agent: string): Promise<void> {
    await expect(this.agentBtn(agent)).toHaveAttribute('aria-pressed', 'true');
  }

  async assertAgentCardState(agent: string, state: string): Promise<void> {
    await expect(this.agentCard(agent)).toHaveAttribute('data-state', state);
  }

  async reset(): Promise<void> {
    await this.newDispatchBtn.click();
    await expect(this.taskInput).toBeVisible({ timeout: 5_000 });
  }
}
