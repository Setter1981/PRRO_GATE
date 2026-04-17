using System.Runtime.InteropServices;

namespace WebCheckServer;

[StructLayout(LayoutKind.Sequential, Size = 1)]
internal struct TypErr
{
	public const int Ok = 0;

	public const int Err = 9999;
}
