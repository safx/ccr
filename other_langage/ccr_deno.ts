#!/usr/bin/env -S deno run --allow-read --allow-env

// Deno standalone version of ccusage statusline command
// Usage: echo '{"session_id":"...", ...}' | deno run --allow-read --allow-env cc_statusline.ts

import { green, yellow, red, magenta, cyan, white, bold } from "https://deno.land/std@0.224.0/fmt/colors.ts";
import { z } from "https://deno.land/x/zod@v3.22.4/mod.ts";
import { join, basename } from "https://deno.land/std@0.224.0/path/mod.ts";
import { exists } from "https://deno.land/std@0.224.0/fs/exists.ts";

// Schema for hook input
const statuslineHookJsonSchema = z.object({
	session_id: z.string(),
	transcript_path: z.string(),
	cwd: z.string(),
	model: z.object({
		id: z.string(),
		display_name: z.string(),
	}),
	workspace: z.object({
		current_dir: z.string(),
		project_dir: z.string(),
	}),
	version: z.string().optional(),
});

// Types
type StatuslineHookJson = z.infer<typeof statuslineHookJsonSchema>;

interface UsageEntry {
	timestamp: string;
	model?: string;
	inputTokens?: number;
	outputTokens?: number;
	cacheCreationTokens?: number;
	cacheReadTokens?: number;
	costUSD?: number;
	// Fields from actual Claude Code JSONL format
	message?: {
		id?: string;
		model?: string;
		usage?: {
			input_tokens?: number;
			output_tokens?: number;
			cache_creation_input_tokens?: number;
			cache_read_input_tokens?: number;
		};
	};
	requestId?: string;
}

// Transcript message structure for parsing JSONL
interface TranscriptMessage {
	type: string;
	message?: {
		usage?: {
			input_tokens?: number;
			output_tokens?: number;
			cache_creation_input_tokens?: number;
			cache_read_input_tokens?: number;
		};
	};
}

interface SessionBlock {
	startTime: Date;
	endTime: Date;
	isActive: boolean;
	costUSD: number;
	entries: UsageEntry[];
}

interface BurnRate {
	tokensPerMinute: number;
	tokensPerMinuteForIndicator: number;
	costPerHour: number;
}

interface ModelPricing {
	input_cost_per_token?: number;
	output_cost_per_token?: number;
	cache_creation_input_token_cost?: number;
	cache_read_input_token_cost?: number;
}

// Model pricing data (from LiteLLM)
const MODEL_PRICING: Record<string, ModelPricing> = {
	"claude-opus-4-1-20250805": {
		input_cost_per_token: 0.000015,
		output_cost_per_token: 0.000075,
		cache_creation_input_token_cost: 0.00001875,
		cache_read_input_token_cost: 0.0000015,
	},
	"claude-sonnet-4-20250514": {
		input_cost_per_token: 0.000003,
		output_cost_per_token: 0.000015,
		cache_creation_input_token_cost: 0.00000375,
		cache_read_input_token_cost: 0.0000003,
	},
	// Fallback for older models
	"claude-3-opus-20240229": {
		input_cost_per_token: 0.000015,
		output_cost_per_token: 0.000075,
		cache_creation_input_token_cost: 0.00001875,
		cache_read_input_token_cost: 0.0000015,
	},
	"claude-3.5-sonnet-20241022": {
		input_cost_per_token: 0.000003,
		output_cost_per_token: 0.000015,
		cache_creation_input_token_cost: 0.00000375,
		cache_read_input_token_cost: 0.0000003,
	},
};

// Get pricing for a model with fallback
function getModelPricing(modelName?: string): ModelPricing | null {
	if (!modelName) return null;
	
	// Direct match
	if (MODEL_PRICING[modelName]) {
		return MODEL_PRICING[modelName];
	}
	
	// Try partial matching
	for (const [key, pricing] of Object.entries(MODEL_PRICING)) {
		if (modelName.includes(key) || key.includes(modelName)) {
			return pricing;
		}
	}
	
	// Default to Opus pricing if no match
	if (modelName.toLowerCase().includes("opus")) {
		return MODEL_PRICING["claude-opus-4-1-20250805"];
	}
	if (modelName.toLowerCase().includes("sonnet")) {
		return MODEL_PRICING["claude-sonnet-4-20250514"];
	}
	
	return null;
}

// Calculate cost from tokens
function calculateCost(
	tokens: {
		input?: number;
		output?: number;
		cacheCreation?: number;
		cacheRead?: number;
	},
	pricing: ModelPricing
): number {
	let cost = 0;
	
	if (tokens.input && pricing.input_cost_per_token) {
		cost += tokens.input * pricing.input_cost_per_token;
	}
	if (tokens.output && pricing.output_cost_per_token) {
		cost += tokens.output * pricing.output_cost_per_token;
	}
	if (tokens.cacheCreation && pricing.cache_creation_input_token_cost) {
		cost += tokens.cacheCreation * pricing.cache_creation_input_token_cost;
	}
	if (tokens.cacheRead && pricing.cache_read_input_token_cost) {
		cost += tokens.cacheRead * pricing.cache_read_input_token_cost;
	}
	
	return cost;
}

// Utility functions
function formatCurrency(amount: number): string {
	return `$${amount.toFixed(2)}`;
}

function formatRemainingTime(remaining: number): string {
	// Handle negative remaining time (block expired)
	if (remaining <= 0) {
		return "Block expired";
	}
	
	const remainingHours = Math.floor(remaining / 60);
	const remainingMins = remaining % 60;
	if (remainingHours > 0) {
		return `${remainingHours}h ${remainingMins}m left`;
	}
	return `${remainingMins}m left`;
}

// Get Claude data directories
function getClaudePaths(): string[] {
	const paths: string[] = [];
	const home = Deno.env.get("HOME") || Deno.env.get("USERPROFILE") || "";
	
	// Check environment variable first
	const customPath = Deno.env.get("CLAUDE_CONFIG_DIR");
	if (customPath) {
		const customPaths = customPath.split(",").map(p => p.trim()).filter(p => p);
		paths.push(...customPaths);
	} else {
		// Default paths
		paths.push(join(home, ".config", "claude"));
		paths.push(join(home, ".claude"));
	}
	
	return paths.filter(p => {
		try {
			const stat = Deno.statSync(p);
			return stat.isDirectory;
		} catch {
			return false;
		}
	});
}

// Load session usage data
async function loadSessionUsageById(sessionId: string): Promise<{ totalCost: number } | null> {
	const claudePaths = getClaudePaths();
	let totalCost = 0;
	let found = false;
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			// Find session file in projects directory
			const projectsPath = join(basePath, "projects");
			if (!await exists(projectsPath)) continue;
			
			for await (const dirEntry of Deno.readDir(projectsPath)) {
				if (!dirEntry.isDirectory) continue;
				
				const sessionFile = join(projectsPath, dirEntry.name, `${sessionId}.jsonl`);
				if (await exists(sessionFile)) {
					const content = await Deno.readTextFile(sessionFile);
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							
							// Deduplication logic (exactly matching ccusage)
							const messageId = entry.message?.id;
							const requestId = entry.requestId;
							
							// Only perform deduplication if BOTH IDs exist
							// If either is missing, skip deduplication (entry is kept)
							if (messageId && requestId) {
								const uniqueHash = `${messageId}:${requestId}`;
								if (processedHashes.has(uniqueHash)) {
									continue; // Skip duplicate
								}
								processedHashes.add(uniqueHash);
							}
							// If either ID is missing, keep the entry (no deduplication check)
							
							// First check for pre-calculated costUSD
							if (entry.costUSD) {
								totalCost += entry.costUSD;
								found = true;
							}
							// Otherwise calculate from usage data
							else if (entry.message?.usage) {
								const usage = entry.message.usage;
								const modelName = entry.message.model || entry.model;
								const pricing = getModelPricing(modelName);
								
								if (pricing) {
									const cost = calculateCost({
										input: usage.input_tokens,
										output: usage.output_tokens,
										cacheCreation: usage.cache_creation_input_tokens,
										cacheRead: usage.cache_read_input_tokens,
									}, pricing);
									
									if (cost > 0) {
										totalCost += cost;
										found = true;
									}
								}
							}
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	return found ? { totalCost } : null;
}

// Load today's usage data
async function loadTodayUsageData(): Promise<number> {
	const claudePaths = getClaudePaths();
	
	// Get user's timezone
	const userTimezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
	
	// Get today's date string in user's timezone (YYYY-MM-DD format)
	const now = new Date();
	const todayDateStr = now.toLocaleDateString('en-CA', { timeZone: userTimezone }); // en-CA gives YYYY-MM-DD
	
	// Calculate the UTC range for this local date
	// This is a simplified approach: check if the entry's local date matches today
	const isToday = (timestamp: string): boolean => {
		const entryDate = new Date(timestamp);
		const entryLocalDateStr = entryDate.toLocaleDateString('en-CA', { timeZone: userTimezone });
		return entryLocalDateStr === todayDateStr;
	};
	
	let totalCost = 0;
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			const projectsPath = join(basePath, "projects");
			if (!await exists(projectsPath)) continue;
			
			for await (const dirEntry of Deno.readDir(projectsPath)) {
				if (!dirEntry.isDirectory) continue;
				
				for await (const file of Deno.readDir(join(projectsPath, dirEntry.name))) {
					if (!file.name.endsWith(".jsonl")) continue;
					
					const filePath = join(projectsPath, dirEntry.name, file.name);
					const content = await Deno.readTextFile(filePath);
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							if (entry.timestamp && isToday(entry.timestamp)) {
								// Create unique hash for deduplication
								const messageId = entry.message?.id;
								const requestId = entry.requestId;
								
								// Only perform deduplication if BOTH IDs exist
								// If either is missing, skip deduplication (entry is kept)
								if (messageId && requestId) {
									const uniqueHash = `${messageId}:${requestId}`;
									if (processedHashes.has(uniqueHash)) {
										continue; // Skip duplicate
									}
									processedHashes.add(uniqueHash);
								}
								// If either ID is missing, keep the entry (no deduplication check)
								
								// Check for pre-calculated costUSD
								if (entry.costUSD) {
									totalCost += entry.costUSD;
								}
								// Otherwise calculate from usage data
								else if (entry.message?.usage) {
									const usage = entry.message.usage;
									const modelName = entry.message.model || entry.model;
									const pricing = getModelPricing(modelName);
									
									if (pricing) {
										const cost = calculateCost({
											input: usage.input_tokens,
											output: usage.output_tokens,
											cacheCreation: usage.cache_creation_input_tokens,
											cacheRead: usage.cache_read_input_tokens,
										}, pricing);
										
										totalCost += cost;
									}
								}
							}
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	return totalCost;
}

// Helper function to floor timestamp to the hour
function floorToHour(timestamp: Date): Date {
	const floored = new Date(timestamp);
	floored.setUTCMinutes(0, 0, 0);
	return floored;
}

// Load active session block
async function loadActiveBlock(): Promise<{ blockInfo: string; burnRateInfo: string, remainingInfo: String }> {
	const claudePaths = getClaudePaths();
	const now = new Date();
	const fiveHoursInMs = 5 * 60 * 60 * 1000;
	
	// Collect all usage entries
	const allEntries: { entry: UsageEntry; time: Date }[] = [];
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			const projectsPath = join(basePath, "projects");
			if (!await exists(projectsPath)) continue;
			
			for await (const dirEntry of Deno.readDir(projectsPath)) {
				if (!dirEntry.isDirectory) continue;
				
				for await (const file of Deno.readDir(join(projectsPath, dirEntry.name))) {
					if (!file.name.endsWith(".jsonl")) continue;
					
					const filePath = join(projectsPath, dirEntry.name, file.name);
					const content = await Deno.readTextFile(filePath);
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							
							// Validate timestamp
							if (!entry.timestamp) {
								continue;
							}
							const entryTime = new Date(entry.timestamp);
							if (isNaN(entryTime.getTime())) {
								continue;
							}
							
							// Skip entries that have neither usage data nor costUSD
							if (!entry.message?.usage && !entry.costUSD) {
								continue;
							}
							
							// Deduplication logic (exactly matching ccusage)
							const messageId = entry.message?.id;
							const requestId = entry.requestId;
							
							// Only perform deduplication if BOTH IDs exist
							// If either is missing, skip deduplication (entry is kept)
							if (messageId && requestId) {
								const uniqueHash = `${messageId}:${requestId}`;
								if (processedHashes.has(uniqueHash)) {
									continue; // Skip duplicate
								}
								processedHashes.add(uniqueHash);
							}
							// If either ID is missing, keep the entry (no deduplication check)
							
							allEntries.push({ entry, time: entryTime });
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	// Sort entries by timestamp
	allEntries.sort((a, b) => a.time.getTime() - b.time.getTime());
	
	// Identify session blocks (exactly matching ccusage logic)
	const blocks: { start: Date; entries: UsageEntry[]; actualEnd: Date }[] = [];
	let currentBlockStart: Date | null = null;
	let currentBlockEntries: UsageEntry[] = [];
	
	for (const { entry, time } of allEntries) {
		if (currentBlockStart === null) {
			// First entry - start a new block (floored to the hour)
			currentBlockStart = floorToHour(time);
			currentBlockEntries = [entry];
		} else {
			// Calculate time since block start (using floored time)
			const timeSinceBlockStart = time.getTime() - currentBlockStart.getTime();
			
			// Get last entry time
			const lastEntry = currentBlockEntries[currentBlockEntries.length - 1];
			if (!lastEntry || !lastEntry.timestamp) {
				continue;
			}
			const lastEntryTime = new Date(lastEntry.timestamp);
			const timeSinceLastEntry = time.getTime() - lastEntryTime.getTime();
			
			// Check if we need to close the current block
			if (timeSinceBlockStart > fiveHoursInMs || timeSinceLastEntry > fiveHoursInMs) {
				// Close current block
				blocks.push({
					start: currentBlockStart,
					entries: currentBlockEntries,
					actualEnd: lastEntryTime
				});
				
				// Note: Could add gap block logic here if timeSinceLastEntry > fiveHoursInMs
				// But for statusline we only care about active blocks
				
				// Start new block (floored to the hour)
				currentBlockStart = floorToHour(time);
				currentBlockEntries = [entry];
			} else {
				// Add to current block
				currentBlockEntries.push(entry);
			}
		}
	}
	
	// Close the last block if any
	if (currentBlockStart !== null && currentBlockEntries.length > 0) {
		const lastEntry = currentBlockEntries[currentBlockEntries.length - 1];
		if (lastEntry && lastEntry.timestamp) {
			blocks.push({
				start: currentBlockStart,
				entries: currentBlockEntries,
				actualEnd: new Date(lastEntry.timestamp)
			});
		}
	}
	
	// Find the active block (exactly matching ccusage isActive logic)
	let activeBlock: { start: Date; entries: UsageEntry[]; actualEnd: Date } | null = null;
	for (const block of blocks) {
		// Block end time is start + 5 hours
		const blockEndTime = new Date(block.start.getTime() + fiveHoursInMs);
		
		// Time since last activity in the block
		const timeSinceLastActivity = now.getTime() - block.actualEnd.getTime();
		
		// Active if: last activity within 5 hours AND current time before block end
		const isActive = timeSinceLastActivity < fiveHoursInMs && now < blockEndTime;
		
		if (isActive) {
			activeBlock = block;
			// Keep looking for the most recent active block
		}
	}
	
	if (!activeBlock) {
		return { blockInfo: "No active block", burnRateInfo: "", remainingInfo: "" };
	}
	
	
	// Calculate total cost for the active block (using auto mode logic like ccusage)
	let totalCost = 0;
	for (const entry of activeBlock.entries) {
		// Auto mode: prefer costUSD if available, otherwise calculate from tokens
		if (entry.costUSD !== undefined && entry.costUSD !== null) {
			// Use pre-calculated costUSD (display mode behavior)
			totalCost += entry.costUSD;
		} else if (entry.message?.usage) {
			// Calculate from usage data (calculate mode behavior)
			const usage = entry.message.usage;
			const modelName = entry.message.model || entry.model;
			const pricing = getModelPricing(modelName);
			
			if (pricing) {
				const cost = calculateCost({
					input: usage.input_tokens,
					output: usage.output_tokens,
					cacheCreation: usage.cache_creation_input_tokens,
					cacheRead: usage.cache_read_input_tokens,
				}, pricing);
				
				totalCost += cost;
			}
		}
		// If both fail, contribute 0 to totalCost (already handled by +=)
	}
	
	// Calculate block end time
	const blockEndTime = new Date(activeBlock.start.getTime() + fiveHoursInMs);
	const remaining = Math.round((blockEndTime.getTime() - now.getTime()) / (1000 * 60));
	
	// Calculate burn rate (exactly matching ccusage calculateBurnRate logic)
	let burnRateInfo = "";
	
	// Check if block has entries (skip if empty or gap block)
	if (activeBlock.entries.length === 0) {
		burnRateInfo = "";
	} else {
		const firstEntry = activeBlock.entries[0];
		const lastEntry = activeBlock.entries[activeBlock.entries.length - 1];
		
		if (!firstEntry?.timestamp || !lastEntry?.timestamp) {
			burnRateInfo = "";
		} else {
			const firstEntryTime = new Date(firstEntry.timestamp);
			const lastEntryTime = new Date(lastEntry.timestamp);
			const durationMinutes = (lastEntryTime.getTime() - firstEntryTime.getTime()) / (1000 * 60);
			
			// Skip if duration is 0 or negative
			if (durationMinutes <= 0) {
				burnRateInfo = "";
			} else {
				// Calculate cost per hour
				const costPerHour = (totalCost / durationMinutes) * 60;
				const costPerHourStr = `${formatCurrency(costPerHour)}/hr`;
				
				// Calculate non-cache tokens for burn rate indicator (matching ccusage)
				let nonCacheTokens = 0;
				for (const entry of activeBlock.entries) {
					if (entry.message?.usage) {
						// Only count input and output tokens, not cache tokens
						nonCacheTokens += (entry.message.usage.input_tokens || 0) + (entry.message.usage.output_tokens || 0);
					}
				}
				const tokensPerMinuteForIndicator = nonCacheTokens / durationMinutes;
				
				// Burn rate coloring based on tokens per minute (exact thresholds from ccusage)
				const coloredBurnRate = tokensPerMinuteForIndicator < 2000
					? green(costPerHourStr)  // Normal (< 2000)
					: tokensPerMinuteForIndicator < 5000
						? yellow(costPerHourStr)  // Moderate (2000-5000)
						: red(costPerHourStr);  // High (> 5000)
				
				burnRateInfo = `${coloredBurnRate}`;
			}
		}
	}
	
	const blockInfo = `${formatCurrency(totalCost)} block`;
	const remainingInfo = magenta(`${formatRemainingTime(remaining)}`);
	return { blockInfo, burnRateInfo, remainingInfo };
}

// Get Git branch name
async function getGitBranch(cwd: string): Promise<string | null> {
	try {
		// Try to read .git/HEAD (fastest method)
		const headPath = join(cwd, ".git", "HEAD");
		const content = await Deno.readTextFile(headPath);
		const trimmedContent = content.trim();
		
		// Parse ref: refs/heads/branch-name format
		const match = trimmedContent.match(/^ref: refs\/heads\/(.+)$/);
		if (match) {
			return match[1];
		}
		
		// For detached HEAD, return first 7 chars of commit hash
		if (trimmedContent.length >= 7 && !trimmedContent.startsWith("ref:")) {
			return trimmedContent.substring(0, 7);
		}
		
		return null;
	} catch {
		// Not a git repository or .git/HEAD not accessible
		return null;
	}
}

// Calculate context tokens from transcript (matching Rust implementation)
async function calculateContextTokens(transcriptPath: string): Promise<string | null> {
	try {
		const content = await Deno.readTextFile(transcriptPath);
		
		// Parse JSONL lines from last to first (most recent usage info)
		const lines = content.trim().split('\n').reverse();
		
		for (const line of lines) {
			const trimmed = line.trim();
			if (!trimmed) continue;
			
			try {
				const msg = JSON.parse(trimmed) as TranscriptMessage;
				
				// Check if this is an assistant message with usage info
				if (msg.type === "assistant" && msg.message?.usage?.input_tokens) {
					const usage = msg.message.usage;
					
					// Calculate total input tokens including cache
					const totalInput = (usage.input_tokens || 0) +
						(usage.cache_creation_input_tokens || 0) +
						(usage.cache_read_input_tokens || 0);
					
					// Calculate percentage (capped at 9999% for display)
					const maxTokens = 200000;
					const percentage = Math.min(Math.floor((totalInput * 100) / maxTokens), 9999);
					
					// Color coding
					const percentageStr = `${percentage}%`;
					const coloredPercentage = percentage < 50
						? green(percentageStr)
						: percentage < 80
							? yellow(percentageStr)
							: red(percentageStr);
					
					// Format with thousands separator
					return `${totalInput.toLocaleString()} (${coloredPercentage})`;
				}
			} catch {
				// Skip invalid lines
			}
		}
		
		// No valid usage information found
		return null;
	} catch {
		return null;
	}
}

// Main function
async function main() {
	// Read input from stdin
	const decoder = new TextDecoder();
	const input = decoder.decode(await Deno.stdin.readable.getReader().read().then(r => r.value || new Uint8Array()));
	const trimmedInput = input.trim();
	
	if (trimmedInput.length === 0) {
		console.log("❌ No input provided");
		Deno.exit(1);
	}
	
	// Parse input
	let hookData: StatuslineHookJson;
	try {
		const parsed = JSON.parse(trimmedInput);
		hookData = statuslineHookJsonSchema.parse(parsed);
	} catch (error) {
		console.log("❌ Invalid input format:", error instanceof Error ? error.message : String(error));
		Deno.exit(1);
	}
	
	// Check Claude paths
	const claudePaths = getClaudePaths();
	if (claudePaths.length === 0) {
		console.log("❌ No Claude data directory found");
		Deno.exit(1);
	}
	
	// Load all data in parallel
	const [sessionData, todayCost, blockData, contextInfo, gitBranch] = await Promise.all([
		loadSessionUsageById(hookData.session_id),
		loadTodayUsageData(),
		loadActiveBlock(),
		calculateContextTokens(hookData.transcript_path),
		getGitBranch(hookData.cwd),
	]);
	
	// Format output
	const currentDir = green(basename(hookData.cwd));
	const colorReset = "\x1b[0m"
	const branchDisplay = gitBranch ? ` ${cyan(`${gitBranch}`)}` : "";
	const modelName = hookData.model.display_name;
	const isOpus = modelName.toLowerCase().includes("opus");
	const coloredModelName = isOpus ? white(modelName) : bold(yellow(modelName));
	const sessionDisplay = sessionData ? formatCurrency(sessionData.totalCost) : "N/A";
	const burnRateInfo = blockData.burnRateInfo ? ` 🔥 ${blockData.burnRateInfo}` : ""
	const remainingInfo = blockData.remainingInfo ? ` ⏰ ${blockData.remainingInfo}` : ""
	const contextInfoStr = contextInfo ? ` ⚖️ ${contextInfo}`: ""
	// Reset color at the beginning to ensure clean output
	const statusLine = `${colorReset}${currentDir}${branchDisplay} 👤 ${coloredModelName}${colorReset}${remainingInfo} 💰 ${formatCurrency(todayCost)} today, ${sessionDisplay} session, ${blockData.blockInfo}${burnRateInfo}${contextInfoStr}`;
	
	console.log(statusLine);
}

// Run main function
if (import.meta.main) {
	await main();
}