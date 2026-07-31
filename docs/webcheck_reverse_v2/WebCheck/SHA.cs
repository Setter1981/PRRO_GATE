using System.IO;
using System.Security.Cryptography;
using System.Text;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SHA
{
	internal static string GenerateSHA256File(string FilePathString, bool Old = true)
	{
		if (Old & (Operators.CompareString(All.tMaxS, "", TextCompare: false) != 0))
		{
			string tMaxS = All.tMaxS;
			All.tMaxS = "";
			return tMaxS;
		}
		if (!File.Exists(FilePathString))
		{
			return "";
		}
		SHA256 sHA = SHA256.Create();
		FileStream fileStream = File.Open(FilePathString, FileMode.Open);
		byte[] array = sHA.ComputeHash(fileStream);
		fileStream.Flush();
		fileStream.Close();
		StringBuilder stringBuilder = new StringBuilder();
		checked
		{
			int num = array.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				stringBuilder.Append(array[i].ToString("X2"));
			}
			return stringBuilder.ToString().ToLower();
		}
	}

	public static string GenerateSHA256String(string inputString)
	{
		SHA256 sHA = SHA256.Create();
		byte[] bytes = Encoding.UTF8.GetBytes(inputString);
		byte[] array = sHA.ComputeHash(bytes);
		StringBuilder stringBuilder = new StringBuilder();
		checked
		{
			int num = array.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				stringBuilder.Append(array[i].ToString("X2"));
			}
			return stringBuilder.ToString();
		}
	}

	public static string GenerateSHA512String(string inputString)
	{
		SHA512 sHA = SHA512.Create();
		byte[] bytes = Encoding.UTF8.GetBytes(inputString);
		byte[] array = sHA.ComputeHash(bytes);
		StringBuilder stringBuilder = new StringBuilder();
		checked
		{
			int num = array.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				stringBuilder.Append(array[i].ToString("X2"));
			}
			return stringBuilder.ToString();
		}
	}
}
