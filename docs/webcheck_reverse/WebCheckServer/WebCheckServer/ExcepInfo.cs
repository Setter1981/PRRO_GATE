using System;
using System.Runtime.InteropServices;

namespace WebCheckServer;

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode, Pack = 8)]
public struct ExcepInfo
{
	public short wCode;

	public short wReserved;

	[MarshalAs(UnmanagedType.BStr)]
	public string bstrSource;

	[MarshalAs(UnmanagedType.BStr)]
	public string bstrDescription;

	[MarshalAs(UnmanagedType.BStr)]
	public string bstrHelpFile;

	public int dwHelpContext;

	public IntPtr pvReserved;

	public IntPtr pfnDereffered;

	public int scode;
}
