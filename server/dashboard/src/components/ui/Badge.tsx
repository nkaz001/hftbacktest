import type { HTMLAttributes } from 'react';

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: 'default' | 'success' | 'warning' | 'destructive' | 'outline';
}

export function Badge({ className = '', variant = 'default', children, ...props }: BadgeProps) {
  const variants = {
    default: 'bg-[hsl(var(--primary))] text-[hsl(var(--primary-foreground))]',
    success: 'bg-[hsl(var(--success))] text-white',
    warning: 'bg-[hsl(var(--warning))] text-black',
    destructive: 'bg-[hsl(var(--destructive))] text-[hsl(var(--destructive-foreground))]',
    outline: 'border border-current bg-transparent',
  };

  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold ${variants[variant]} ${className}`}
      {...props}
    >
      {children}
    </span>
  );
}
