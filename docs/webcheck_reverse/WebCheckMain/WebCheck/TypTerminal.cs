using System.Runtime.InteropServices;

namespace WebCheck;

[StructLayout(LayoutKind.Sequential, Size = 1)]
public struct TypTerminal
{
	public const int DEMO = 999;

	public const int Err = 0;

	public const int PrivatCOM = 1;

	public const int PrivatIP = 2;

	public const int PrivatOldCOM = 3;

	public const int PrivatOldIP = 4;

	public const int BposCOM = 5;

	public const int BposIP = 6;

	public const int PosAPI = 7;
}
