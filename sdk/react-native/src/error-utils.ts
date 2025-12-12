/**
 * Error Handling Utilities
 *
 * Provides structured error handling with user-friendly messages.
 */

import { ICNError, ICNErrorType } from './types';

/**
 * Parse HTTP error response into structured ICNError
 */
export async function parseError(response: Response): Promise<ICNError> {
  let message = 'An unexpected error occurred';
  
  try {
    const text = await response.text();
    message = text || response.statusText;
  } catch {
    message = response.statusText;
  }

  const type = getErrorType(response.status, message);
  const userMessage = getUserFriendlyMessage(type, message);
  const retryable = isRetryable(type, response.status);

  return {
    type,
    message,
    userMessage,
    retryable,
  };
}

/**
 * Create ICNError from JavaScript Error
 */
export function createError(error: Error, type?: ICNErrorType): ICNError {
  const errorType = type || ICNErrorType.Unknown;
  const userMessage = getUserFriendlyMessage(errorType, error.message);

  return {
    type: errorType,
    message: error.message,
    userMessage,
    retryable: errorType === ICNErrorType.Network,
    originalError: error,
  };
}

/**
 * Determine error type from HTTP status and message
 */
function getErrorType(status: number, message: string): ICNErrorType {
  if (status === 401 || status === 403) {
    return ICNErrorType.Authentication;
  }
  if (status === 404) {
    return ICNErrorType.NotFound;
  }
  if (status === 429) {
    return ICNErrorType.RateLimit;
  }
  if (status === 400) {
    return ICNErrorType.Validation;
  }
  if (status >= 500) {
    return ICNErrorType.ServerError;
  }
  if (message.toLowerCase().includes('network') || message.toLowerCase().includes('fetch')) {
    return ICNErrorType.Network;
  }
  return ICNErrorType.Unknown;
}

/**
 * Get user-friendly error message
 */
function getUserFriendlyMessage(type: ICNErrorType, originalMessage: string): string {
  switch (type) {
    case ICNErrorType.Network:
      return 'Unable to connect. Please check your internet connection and try again.';
    case ICNErrorType.Authentication:
      return 'Your session has expired. Please log in again.';
    case ICNErrorType.Validation:
      return 'Please check your input and try again.';
    case ICNErrorType.NotFound:
      return 'The requested resource was not found.';
    case ICNErrorType.RateLimit:
      return 'Too many requests. Please wait a moment and try again.';
    case ICNErrorType.ServerError:
      return 'Server error. Please try again later.';
    case ICNErrorType.Unknown:
    default:
      // Try to extract meaningful part from error message
      if (originalMessage.length < 100 && !originalMessage.includes('http')) {
        return originalMessage;
      }
      return 'Something went wrong. Please try again.';
  }
}

/**
 * Determine if error is retryable
 */
function isRetryable(type: ICNErrorType, status?: number): boolean {
  // Network errors are always retryable
  if (type === ICNErrorType.Network) {
    return true;
  }

  // Server errors (5xx) are retryable
  if (type === ICNErrorType.ServerError) {
    return true;
  }

  // Rate limits are retryable after delay
  if (type === ICNErrorType.RateLimit) {
    return true;
  }

  // Client errors (4xx) are generally not retryable
  return false;
}

/**
 * Check if error is a network error
 */
export function isNetworkError(error: ICNError | Error): boolean {
  if ('type' in error) {
    return error.type === ICNErrorType.Network;
  }
  
  const message = error.message.toLowerCase();
  return (
    message.includes('network') ||
    message.includes('fetch') ||
    message.includes('connection') ||
    message.includes('timeout')
  );
}

/**
 * Check if error requires authentication
 */
export function isAuthError(error: ICNError | Error): boolean {
  if ('type' in error) {
    return error.type === ICNErrorType.Authentication;
  }
  
  const message = error.message.toLowerCase();
  return message.includes('unauthorized') || message.includes('forbidden');
}
