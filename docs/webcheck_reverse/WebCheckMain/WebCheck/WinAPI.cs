using System;
using System.Runtime.InteropServices;

namespace WebCheck;

internal class WinAPI
{
	public const int SW_HIDE = 0;

	[DllImport("User32.dll")]
	public static extern IntPtr SetParent(IntPtr hWndChild, IntPtr hWndNewParent);
}
