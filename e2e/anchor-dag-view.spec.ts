import { test, expect } from '@playwright/test';

test('Anchor DAG View loads and displays correctly', async ({ page }) => {
  // Navigate to the wallet app
  await page.goto('http://localhost:3000');
  
  // Import sample anchor credential
  await page.click('text=Import');
  await page.setInputFiles('input[type="file"]', './fixtures/anchor.cred.json');
  await page.click('text=Import Credential');
  
  // Wait for success message
  await expect(page.locator('text=Credential imported successfully')).toBeVisible();
  
  // Import sample receipt credentials
  await page.click('text=Import');
  await page.setInputFiles('input[type="file"]', './fixtures/receipt1.vc.json');
  await page.click('text=Import Credential');
  await expect(page.locator('text=Credential imported successfully')).toBeVisible();
  
  await page.click('text=Import');
  await page.setInputFiles('input[type="file"]', './fixtures/receipt2.vc.json');
  await page.click('text=Import Credential');
  await expect(page.locator('text=Credential imported successfully')).toBeVisible();
  
  // Navigate to DAG view
  await page.click('text=Credentials');
  await page.click('text=View DAG');
  
  // Check that the Anchor DAG View loaded
  await expect(page.locator('text=Epoch 2025-Q2')).toBeVisible();
  await expect(page.locator('text=Palmyra Federation')).toBeVisible();
  
  // Check that the anchor node is displayed
  const anchorNode = page.locator('[data-testid="anchor-node"]');
  await expect(anchorNode).toBeVisible();
  
  // Check that child receipts are displayed
  await expect(page.locator('text=Receipt 1')).toBeVisible();
  await expect(page.locator('text=Receipt 2')).toBeVisible();
  
  // Test interaction - click on the anchor node
  await anchorNode.click();
  
  // Verify that the tooltip appears with details
  await expect(page.locator('text=DAG Root:')).toBeVisible();
  await expect(page.locator('text=Merkle root bf3')).toBeVisible();
  
  // Test filtering
  await page.click('text=Filter');
  await page.click('text=Anchor Nodes Only');
  
  // Verify only anchor node is shown
  await expect(anchorNode).toBeVisible();
  await expect(page.locator('text=Receipt 1')).not.toBeVisible();
  
  // Reset filter
  await page.click('text=Show All');
  await expect(page.locator('text=Receipt 1')).toBeVisible();
});

test('Import and auto-detect DAG anchor relationship', async ({ page }) => {
  // Navigate to the wallet app
  await page.goto('http://localhost:3000');
  
  // Import receipt with dagAnchor first (without importing the anchor)
  await page.click('text=Import');
  await page.setInputFiles('input[type="file"]', './fixtures/receipt1.vc.json');
  await page.click('text=Import Credential');
  await expect(page.locator('text=Credential imported successfully')).toBeVisible();
  
  // Navigate to the credential and view it
  await page.click('text=Credentials');
  await page.click('text=Receipt 1');
  
  // We should see a prompt to import the anchor
  await expect(page.locator('text=AnchorCredential not found – import now?')).toBeVisible();
  
  // Click to import anchor
  await page.click('text=Import Anchor');
  
  // Choose file
  await page.setInputFiles('input[type="file"]', './fixtures/anchor.cred.json');
  await page.click('text=Import Anchor Credential');
  
  // Should now see both in the DAG view with a connection
  await expect(page.locator('text=Epoch 2025-Q2')).toBeVisible();
  await expect(page.locator('text=Receipt 1')).toBeVisible();
  
  // Check that they're connected with a line
  // Note: Testing SVG connections is complex, but we can check if both elements
  // are in the viewport with the expected relationship
  const anchorPos = await page.locator('[data-testid="anchor-node"]').boundingBox();
  const receiptPos = await page.locator('text=Receipt 1').boundingBox();
  
  // The anchor should be positioned above the receipt
  expect(anchorPos?.y).toBeLessThan(receiptPos?.y);
}); 