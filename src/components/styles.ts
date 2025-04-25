import { CSSProperties } from 'react';

// Common tooltip styles used across components
export const tooltipStyles: CSSProperties = {
  backgroundColor: 'white',
  border: '1px solid #ddd',
  borderRadius: '4px',
  padding: '8px',
  fontSize: '12px',
  pointerEvents: 'none',
  boxShadow: '0 2px 4px rgba(0, 0, 0, 0.1)',
  zIndex: 100,
  maxWidth: '250px',
};

// Button styles
export const buttonStyles: CSSProperties = {
  padding: '8px 16px',
  borderRadius: '4px',
  border: 'none',
  cursor: 'pointer',
  fontSize: '14px',
  fontWeight: 500,
  transition: 'background-color 0.2s ease',
};

export const primaryButtonStyles: CSSProperties = {
  ...buttonStyles,
  backgroundColor: '#1976d2',
  color: 'white',
};

export const secondaryButtonStyles: CSSProperties = {
  ...buttonStyles,
  backgroundColor: '#f5f5f5',
  color: '#333',
  border: '1px solid #ddd',
};

// Card styles
export const cardStyles: CSSProperties = {
  backgroundColor: 'white',
  borderRadius: '8px',
  boxShadow: '0 2px 8px rgba(0, 0, 0, 0.1)',
  padding: '16px',
  marginBottom: '16px',
};

// Badge styles
export const badgeStyles: CSSProperties = {
  display: 'inline-block',
  padding: '4px 8px',
  borderRadius: '12px',
  fontSize: '11px',
  fontWeight: 500,
};

export const statusBadgeStyles = (status: string): CSSProperties => {
  const colors: Record<string, { bg: string; color: string }> = {
    success: { bg: '#e6f7ed', color: '#0d904b' },
    warning: { bg: '#fff8e6', color: '#b36500' },
    error: { bg: '#ffebee', color: '#d32f2f' },
    info: { bg: '#e3f2fd', color: '#1976d2' },
    pending: { bg: '#f5f5f5', color: '#616161' },
  };
  
  const style = colors[status] || colors.info;
  
  return {
    ...badgeStyles,
    backgroundColor: style.bg,
    color: style.color,
  };
};

// Layout styles
export const flexRowStyles: CSSProperties = {
  display: 'flex',
  flexDirection: 'row',
  alignItems: 'center',
};

export const flexColumnStyles: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
};

export const spaceBetweenStyles: CSSProperties = {
  ...flexRowStyles,
  justifyContent: 'space-between',
};

// Form styles
export const formControlStyles: CSSProperties = {
  marginBottom: '16px',
};

export const inputStyles: CSSProperties = {
  padding: '8px 12px',
  borderRadius: '4px',
  border: '1px solid #ddd',
  fontSize: '14px',
  width: '100%',
};

export const labelStyles: CSSProperties = {
  display: 'block',
  marginBottom: '4px',
  fontSize: '14px',
  fontWeight: 500,
};

// Table styles
export const tableStyles: CSSProperties = {
  width: '100%',
  borderCollapse: 'collapse',
};

export const tableCellStyles: CSSProperties = {
  padding: '8px 12px',
  borderBottom: '1px solid #eee',
  fontSize: '14px',
  textAlign: 'left',
};

export const tableHeaderStyles: CSSProperties = {
  ...tableCellStyles,
  fontWeight: 500,
  backgroundColor: '#f5f5f5',
}; 