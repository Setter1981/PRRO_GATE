using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[StandardModule]
internal sealed class Secondary
{
	internal static string[] SendMail = new string[6];

	internal static string[] LastCheckLine;

	internal static int CountLine;

	internal static void SetSizeLastCheckLine(int size)
	{
		CountLine = size;
		LastCheckLine = new string[checked(CountLine + 1)];
	}

	internal static void SetLastCheckLine(int index, string line)
	{
		if (index <= CountLine)
		{
			LastCheckLine[index] = line;
		}
	}

	internal static string GetCheckLine(int index)
	{
		if (index <= CountLine)
		{
			return LastCheckLine[index];
		}
		return "";
	}
}
