using System;
using System.Text;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Coding
{
	private const int KeyCod = 2;

	internal string Cod(string str)
	{
		byte[] bytes = Encoding.Default.GetBytes(str);
		string text = "";
		checked
		{
			int num = bytes.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				text += BytyToStr3(bytes[i]);
			}
			text = Rot(text, 2, cod: true);
			return Revers(text, cod: true);
		}
	}

	internal string DeCod(string str)
	{
		checked
		{
			string result;
			try
			{
				str = str.Trim();
				str = Revers(str, cod: false);
				str = Rot(str, 2, cod: false);
				byte[] array = new byte[unchecked(str.Length / 3) - 1 + 1];
				int num = unchecked(str.Length / 3) - 1;
				for (int i = 0; i <= num; i++)
				{
					string value = Conversions.ToString(str[3 * i]) + Conversions.ToString(str[3 * i + 1]) + Conversions.ToString(str[3 * i + 2]);
					array[i] = Conversions.ToByte(value);
				}
				result = Encoding.Default.GetString(array);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("DeCod", "Ошибка расшифровки пароля оператора", "Пароль в таблице OPERATORS содержит ошибку");
				result = "";
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string Revers(string s, bool cod)
	{
		string text = "";
		int num = 0;
		if (cod)
		{
			Random random = new Random();
			s = s + random.Next(10) + random.Next(10) + random.Next(10) + random.Next(10);
			num = 0;
		}
		else
		{
			num = 4;
		}
		checked
		{
			int num2 = s.Length - 1;
			int num3 = num;
			for (int i = num2; i >= num3; i += -1)
			{
				text += s[i];
			}
			return text;
		}
	}

	private string Rot(string s, int r, bool cod)
	{
		string text = "";
		checked
		{
			int num = s.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				text = ((!cod) ? (text + RotD(s[i].ToString(), s.Length + i + r)) : (text + RotC(s[i].ToString(), s.Length + i + r)));
			}
			return text;
		}
	}

	private string RotC(string s, int r)
	{
		int num = Conversions.ToInteger(s);
		checked
		{
			for (int i = 0; i <= r; i++)
			{
				num++;
				if (num > 9)
				{
					num = 0;
				}
			}
			return num.ToString();
		}
	}

	private string RotD(string s, int r)
	{
		int num = Conversions.ToInteger(s);
		checked
		{
			for (int i = 0; i <= r; i++)
			{
				num--;
				if (num < 0)
				{
					num = 9;
				}
			}
			return num.ToString();
		}
	}

	private string BytyToStr3(byte b)
	{
		string text = b.ToString();
		for (int i = text.Length; i <= 2; i = checked(i + 1))
		{
			text = "0" + text;
		}
		return text;
	}
}
