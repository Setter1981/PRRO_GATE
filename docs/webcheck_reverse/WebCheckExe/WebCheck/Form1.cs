using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;
using WebCheck.My.Resources;

namespace WebCheck;

[DesignerGenerated]
internal class Form1 : Form
{
	private bool NewNumber;

	private bool NewShift;

	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("FNs")]
	private ListBox _FNs;

	[CompilerGenerated]
	[AccessedThroughProperty("ОПрограммеToolStripMenuItem")]
	private ToolStripMenuItem _ОПрограммеToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЛицензииToolStripMenuItem")]
	private ToolStripMenuItem _ЛицензииToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("СоздатьНовуюPROToolStripMenuItem")]
	private ToolStripMenuItem _СоздатьНовуюPROToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("TC")]
	private TabControl _TC;

	[CompilerGenerated]
	[AccessedThroughProperty("ПереглядЗмінІЧеківToolStripMenuItem")]
	private ToolStripMenuItem _ПереглядЗмінІЧеківToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ДрукЧеківToolStripMenuItem")]
	private ToolStripMenuItem _ДрукЧеківToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ОператориToolStripMenuItem")]
	private ToolStripMenuItem _ОператориToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("TB")]
	private TextBox _TB;

	[CompilerGenerated]
	[AccessedThroughProperty("Button1")]
	private Button _Button1;

	[CompilerGenerated]
	[AccessedThroughProperty("Button2")]
	private Button _Button2;

	[CompilerGenerated]
	[AccessedThroughProperty("Button3")]
	private Button _Button3;

	[CompilerGenerated]
	[AccessedThroughProperty("Button4")]
	private Button _Button4;

	[CompilerGenerated]
	[AccessedThroughProperty("Button5")]
	private Button _Button5;

	[CompilerGenerated]
	[AccessedThroughProperty("Button6")]
	private Button _Button6;

	[CompilerGenerated]
	[AccessedThroughProperty("Button8")]
	private Button _Button8;

	[CompilerGenerated]
	[AccessedThroughProperty("Button7")]
	private Button _Button7;

	[CompilerGenerated]
	[AccessedThroughProperty("ВидалитиДемонстраційнийНомерToolStripMenuItem")]
	private ToolStripMenuItem _ВидалитиДемонстраційнийНомерToolStripMenuItem;

	internal virtual ListBox FNs
	{
		[CompilerGenerated]
		get
		{
			return _FNs;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = FNs_SelectedIndexChanged;
			ListBox fNs = _FNs;
			if (fNs != null)
			{
				fNs.SelectedIndexChanged -= eventHandler;
			}
			_FNs = value;
			fNs = _FNs;
			if (fNs != null)
			{
				fNs.SelectedIndexChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Pro")]
	internal virtual Panel Pro
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("StText")]
	internal virtual TextBox StText
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ОПрограммеToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОПрограммеToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ОПрограммеToolStripMenuItem_Click;
			ToolStripMenuItem оПрограммеToolStripMenuItem = _ОПрограммеToolStripMenuItem;
			if (оПрограммеToolStripMenuItem != null)
			{
				((ToolStripItem)оПрограммеToolStripMenuItem).Click -= eventHandler;
			}
			_ОПрограммеToolStripMenuItem = value;
			оПрограммеToolStripMenuItem = _ОПрограммеToolStripMenuItem;
			if (оПрограммеToolStripMenuItem != null)
			{
				((ToolStripItem)оПрограммеToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ЛицензииToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЛицензииToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ЛицензииToolStripMenuItem_Click;
			ToolStripMenuItem лицензииToolStripMenuItem = _ЛицензииToolStripMenuItem;
			if (лицензииToolStripMenuItem != null)
			{
				((ToolStripItem)лицензииToolStripMenuItem).Click -= eventHandler;
			}
			_ЛицензииToolStripMenuItem = value;
			лицензииToolStripMenuItem = _ЛицензииToolStripMenuItem;
			if (лицензииToolStripMenuItem != null)
			{
				((ToolStripItem)лицензииToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem СоздатьНовуюPROToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _СоздатьНовуюPROToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = СоздатьНовуюPROToolStripMenuItem_Click;
			ToolStripMenuItem создатьНовуюPROToolStripMenuItem = _СоздатьНовуюPROToolStripMenuItem;
			if (создатьНовуюPROToolStripMenuItem != null)
			{
				((ToolStripItem)создатьНовуюPROToolStripMenuItem).Click -= eventHandler;
			}
			_СоздатьНовуюPROToolStripMenuItem = value;
			создатьНовуюPROToolStripMenuItem = _СоздатьНовуюPROToolStripMenuItem;
			if (создатьНовуюPROToolStripMenuItem != null)
			{
				((ToolStripItem)создатьНовуюPROToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual TabControl TC
	{
		[CompilerGenerated]
		get
		{
			return _TC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			TabControlEventHandler val = new TabControlEventHandler(TC_Selected);
			TabControl tC = _TC;
			if (tC != null)
			{
				tC.Selected -= val;
			}
			_TC = value;
			tC = _TC;
			if (tC != null)
			{
				tC.Selected += val;
			}
		}
	}

	[field: AccessedThroughProperty("TabPage2")]
	internal virtual TabPage TabPage2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ПереглядЗмінІЧеківToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ПереглядЗмінІЧеківToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ПереглядЗмінІЧеківToolStripMenuItem_Click;
			ToolStripMenuItem переглядЗмінІЧеківToolStripMenuItem = _ПереглядЗмінІЧеківToolStripMenuItem;
			if (переглядЗмінІЧеківToolStripMenuItem != null)
			{
				((ToolStripItem)переглядЗмінІЧеківToolStripMenuItem).Click -= eventHandler;
			}
			_ПереглядЗмінІЧеківToolStripMenuItem = value;
			переглядЗмінІЧеківToolStripMenuItem = _ПереглядЗмінІЧеківToolStripMenuItem;
			if (переглядЗмінІЧеківToolStripMenuItem != null)
			{
				((ToolStripItem)переглядЗмінІЧеківToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ДрукЧеківToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДрукЧеківToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ДрукЧеківToolStripMenuItem_Click;
			ToolStripMenuItem друкЧеківToolStripMenuItem = _ДрукЧеківToolStripMenuItem;
			if (друкЧеківToolStripMenuItem != null)
			{
				((ToolStripItem)друкЧеківToolStripMenuItem).Click -= eventHandler;
			}
			_ДрукЧеківToolStripMenuItem = value;
			друкЧеківToolStripMenuItem = _ДрукЧеківToolStripMenuItem;
			if (друкЧеківToolStripMenuItem != null)
			{
				((ToolStripItem)друкЧеківToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ОператориToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОператориToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ОператориToolStripMenuItem_Click;
			ToolStripMenuItem операториToolStripMenuItem = _ОператориToolStripMenuItem;
			if (операториToolStripMenuItem != null)
			{
				((ToolStripItem)операториToolStripMenuItem).Click -= eventHandler;
			}
			_ОператориToolStripMenuItem = value;
			операториToolStripMenuItem = _ОператориToolStripMenuItem;
			if (операториToolStripMenuItem != null)
			{
				((ToolStripItem)операториToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ЗакритиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЗакритиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click -= eventHandler;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TabPage3")]
	internal virtual TabPage TabPage3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabPage4")]
	internal virtual TabPage TabPage4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabPage5")]
	internal virtual TabPage TabPage5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabPage6")]
	internal virtual TabPage TabPage6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox TB
	{
		[CompilerGenerated]
		get
		{
			return _TB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TB_TextChanged;
			TextBox tB = _TB;
			if (tB != null)
			{
				((Control)tB).TextChanged -= eventHandler;
			}
			_TB = value;
			tB = _TB;
			if (tB != null)
			{
				((Control)tB).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TabPage1")]
	internal virtual TabPage TabPage1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("StText1")]
	internal virtual TextBox StText1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox1")]
	internal virtual TextBox TextBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button1
	{
		[CompilerGenerated]
		get
		{
			return _Button1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button1_Click;
			Button button = _Button1;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button1 = value;
			button = _Button1;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PictureBox1")]
	internal virtual PictureBox PictureBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PictureBox2")]
	internal virtual PictureBox PictureBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox2")]
	internal virtual TextBox TextBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button2
	{
		[CompilerGenerated]
		get
		{
			return _Button2;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button2_Click;
			Button button = _Button2;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button2 = value;
			button = _Button2;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PictureBox3")]
	internal virtual PictureBox PictureBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox3")]
	internal virtual TextBox TextBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button3
	{
		[CompilerGenerated]
		get
		{
			return _Button3;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button3_Click;
			Button button = _Button3;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button3 = value;
			button = _Button3;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PictureBox4")]
	internal virtual PictureBox PictureBox4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox4")]
	internal virtual TextBox TextBox4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button4
	{
		[CompilerGenerated]
		get
		{
			return _Button4;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button4_Click;
			Button button = _Button4;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button4 = value;
			button = _Button4;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PictureBox5")]
	internal virtual PictureBox PictureBox5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox5")]
	internal virtual TextBox TextBox5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button5
	{
		[CompilerGenerated]
		get
		{
			return _Button5;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button5_Click;
			Button button = _Button5;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button5 = value;
			button = _Button5;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PictureBox6")]
	internal virtual PictureBox PictureBox6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBox6")]
	internal virtual TextBox TextBox6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Button6
	{
		[CompilerGenerated]
		get
		{
			return _Button6;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button6_Click;
			Button button = _Button6;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button6 = value;
			button = _Button6;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	internal virtual Button Button8
	{
		[CompilerGenerated]
		get
		{
			return _Button8;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button8_Click;
			Button button = _Button8;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button8 = value;
			button = _Button8;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	internal virtual Button Button7
	{
		[CompilerGenerated]
		get
		{
			return _Button7;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button7_Click;
			Button button = _Button7;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button7 = value;
			button = _Button7;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ВидалитиДемонстраційнийНомерToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВидалитиДемонстраційнийНомерToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ВидалитиДемонстраційнийНомерToolStripMenuItem_Click;
			ToolStripMenuItem видалитиДемонстраційнийНомерToolStripMenuItem = _ВидалитиДемонстраційнийНомерToolStripMenuItem;
			if (видалитиДемонстраційнийНомерToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиДемонстраційнийНомерToolStripMenuItem).Click -= eventHandler;
			}
			_ВидалитиДемонстраційнийНомерToolStripMenuItem = value;
			видалитиДемонстраційнийНомерToolStripMenuItem = _ВидалитиДемонстраційнийНомерToolStripMenuItem;
			if (видалитиДемонстраційнийНомерToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиДемонстраційнийНомерToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	public Form1()
	{
		((Form)this).Load += Form1_Load;
		NewNumber = false;
		NewShift = false;
		InitializeComponent();
	}

	private void Form1_Load(object sender, EventArgs e)
	{
		((Control)Pro).Left = 20;
		((Control)Pro).Top = 21;
		((Control)Pro).Visible = false;
		((Control)FNs).Enabled = false;
		Zapolnit();
		Blokirovka();
	}

	private void Zapolnit()
	{
		WebCheck.IniHGB f = WebCheck.All.f;
		FNs.Items.Clear();
		string text = "";
		int num = f.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			if (Operators.CompareString(f.NameFn(i).Trim().ToLower(), "del", false) == 0 || Operators.CompareString(f.NameFn(i).Trim().ToLower(), "", false) == 0)
			{
				continue;
			}
			text = f.NameFn(i);
			FNs.Items.Add((object)text);
			if (Operators.CompareString(text.Trim(), "7000000512", false) == 0)
			{
				NewNumber = true;
				WebCheck.All.FN = "7000000512";
				if (!WebCheck.All.status)
				{
					WebCheck.All.WC.Initialization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
					WebCheck.All.status = true;
					Blokirovka();
					TC.SelectedIndex = 1;
					TextBox1.Text = "Вітаємо! Тестова база даних усмешно створена!";
				}
			}
		}
		f = null;
	}

	private void Blokirovka()
	{
		TextBox3.Text = "Після того, як Ви створили базу і відкрили зміну, можете фіскалізувати чеки.\r\nНатисніть на кнопку.";
		TextBox4.Text = "Натиснувши на кнопку Ви закриєте зміну і створите Z звіт.";
		TextBox5.Text = "Для перегляду змін і чеків натисніть кнопку.";
		TextBox6.Text = "Програмний Реєстратор Розрахункових Операцій";
		NewShift = OpenShift();
		if (NewNumber)
		{
			TC.TabPages[0].Enabled = false;
			TextBox1.Text = "Тестова база вже створена.";
			if (NewShift)
			{
				TextBox2.Text = "Вже є відкрита зміна, можна працювати з чеками.";
				TC.TabPages[1].Enabled = false;
				TC.TabPages[2].Enabled = true;
				TC.TabPages[3].Enabled = true;
				TC.TabPages[4].Enabled = true;
			}
			else
			{
				TextBox2.Text = "Перед тим, як почати роботу з чеками необхідно відкрити зміну.\r\nНатисніть кнопку...";
				TextBox3.Text = "Перед тим, як почати роботу з чеками необхідно відкрити зміну.\r\nНатисніть кнопку...";
				TC.TabPages[1].Enabled = true;
				TC.TabPages[2].Enabled = false;
				TC.TabPages[3].Enabled = false;
				TC.TabPages[4].Enabled = false;
			}
		}
		else
		{
			TextBox1.Text = "Для початку роботи необхідно створити базу даних.\r\nНатисніть на кнопку і створіть тестову базу, з якої Ви зможете продовжити знайомство з ПРРО...";
			TextBox2.Text = "Для початку роботи необхідно створити базу даних.\r\nНатисніть на кнопку і створіть тестову базу, з якої Ви зможете продовжити знайомство з ПРРО...";
			TextBox3.Text = "Для початку роботи необхідно створити базу даних.\r\nНатисніть на кнопку і створіть тестову базу, з якої Ви зможете продовжити знайомство з ПРРО...";
			TextBox4.Text = "Для початку роботи необхідно створити базу даних.\r\nНатисніть на кнопку і створіть тестову базу, з якої Ви зможете продовжити знайомство з ПРРО...";
			TextBox5.Text = "Для початку роботи необхідно створити базу даних.\r\nНатисніть на кнопку і створіть тестову базу, з якої Ви зможете продовжити знайомство з ПРРО...";
			TC.TabPages[0].Enabled = true;
			TC.TabPages[1].Enabled = false;
			TC.TabPages[2].Enabled = false;
			TC.TabPages[3].Enabled = false;
			TC.TabPages[4].Enabled = false;
		}
	}

	private bool OpenShift()
	{
		bool result;
		try
		{
			WebCheck.All.FN = Conversions.ToString(7000000512L);
			WebCheck.All.WC.GetCurrentStatus("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
			string sXML = WebCheck.All.WC.StatusBarXML();
			string parametrToString = GetParametrToString(sXML, "shiftnumber");
			result = Versioned.IsNumeric((object)parametrToString) && Conversions.ToInteger(parametrToString) > 0;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public string GetParametrToString(string sXML, string name, string knot = "OutputParameters/Parameters", bool RegUpLow = false)
	{
		string text = "";
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			if (!RegUpLow)
			{
				sXML = sXML.ToLower();
				name = name.Trim().ToLower();
				knot = knot.Trim().ToLower();
			}
			xmlDocument.LoadXml(sXML.Trim());
			name = name.Trim();
			knot = knot.Trim();
			text = xmlDocument.SelectSingleNode("/" + knot + "/@" + name).Value;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "";
			ProjectData.ClearProjectError();
		}
		return text;
	}

	private void ОПрограммеToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0005: Unknown result type (might be due to invalid IL or missing references)
		((Form)new Form0()).ShowDialog();
	}

	private void FNs_SelectedIndexChanged(object sender, EventArgs e)
	{
		if (FNs.Items.Count < 1 || Conversions.ToBoolean(Operators.AndObject((object)WebCheck.All.status, Operators.CompareObjectEqual((object)WebCheck.All.FN.Trim(), NewLateBinding.LateGet(FNs.SelectedItem, (Type)null, "trim", new object[0], (string[])null, (Type[])null, (bool[])null), false))))
		{
			return;
		}
		string fN = WebCheck.All.FN;
		WebCheck.All.FN = FNs.SelectedItem.ToString().Trim();
		if (WebCheck.All.FN.Trim().Length == 10)
		{
			WebCheck.All.WC.Finalization("<InputParameters><Parameters FN='" + fN + "' OperatorID='1111111111'/></InputParameters>");
			if (WebCheck.All.WC.Initialization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>"))
			{
				WebCheck.All.status = true;
				WebCheck.All.WC.GetCurrentStatus("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
				StText.Text = WebCheck.All.WC.StatusBarXML();
				WebCheck.All.WC.GetSetingsRRO("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
				StText1.Text = WebCheck.All.WC.StatusBarXML();
				TB.Text = WebCheck.All.FN;
			}
			else
			{
				StText.Text = "ПОМИЛКА";
				StText1.Text = "ПОМИЛКА";
				TB.Text = "ПОМИЛКА";
			}
		}
	}

	private void СоздатьНовуюPROToolStripMenuItem_Click(object sender, EventArgs e)
	{
		WebCheck.All.WC.ShowWizardNewProDemo();
	}

	private void ПереглядЗмінІЧеківToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status)
		{
			WebCheck.All.WC.ShowReports();
		}
	}

	private void TB_TextChanged(object sender, EventArgs e)
	{
		if (Operators.CompareString(TB.Text.Trim(), "OpenPro", false) == 0)
		{
			TB.Text = "";
			((Control)FNs).Enabled = true;
			((Control)Pro).Visible = true;
			if (Operators.CompareString(StText.Text.Trim(), "", false) == 0)
			{
				WebCheck.All.WC.GetCurrentStatus("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
				StText.Text = WebCheck.All.WC.StatusBarXML();
			}
			if (Operators.CompareString(StText1.Text.Trim(), "", false) == 0)
			{
				WebCheck.All.WC.GetSetingsRRO("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
				StText1.Text = WebCheck.All.WC.StatusBarXML();
			}
			((Control)TextBox6).Visible = false;
			((Control)Button6).Visible = false;
			((Control)Button7).Visible = false;
			((Control)Button8).Visible = false;
			((Control)PictureBox6).Visible = false;
			TC.SelectedIndex = 5;
			TB.Text = WebCheck.All.FN;
		}
		else if (Operators.CompareString(TB.Text.Trim().ToLower(), "0000000000", false) == 0)
		{
			TB.Text = "";
			((Control)FNs).Enabled = false;
			((Control)Pro).Visible = false;
			TC.SelectedIndex = 0;
			((Control)TextBox6).Visible = true;
			((Control)Button6).Visible = true;
			((Control)Button7).Visible = true;
			((Control)Button8).Visible = true;
			((Control)PictureBox6).Visible = true;
		}
	}

	private void ОператориToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status)
		{
			WebCheck.All.WC.ShowOperators();
		}
	}

	private void ДрукЧеківToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status)
		{
			WebCheck.All.WC.ShowPrintByCheckFn();
		}
	}

	private void ЛицензииToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status)
		{
			WebCheck.All.WC.ShowLicenseCheck();
		}
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void Button1_Click(object sender, EventArgs e)
	{
		if (!WebCheck.All.status)
		{
			WebCheck.All.WC.ShowWizardNewProDemo();
			Application.DoEvents();
			Zapolnit();
		}
	}

	private void Button2_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status && !NewShift)
		{
			WebCheck.All.FN = "7000000512";
			if (WebCheck.All.WC.OpenShift("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>"))
			{
				NewShift = true;
				Blokirovka();
				TC.SelectedIndex = 2;
				TextBox2.Text = "Вітаємо! Зміна успішно відкрита. Можна працювати з чеками.";
			}
		}
	}

	private void Button3_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status && NewShift)
		{
			WebCheck.All.FN = "7000000512";
			string strXML = "<?xml version='1.0' encoding='windows-1251'?>\r\n<check number='0' fn='7000000512' operationtype='0' uuid='b5bdcebe-032b-4E22-b908-a0c63fcf1af5'>\r\n <checkhead>\r\n  <ver>1</ver>\r\n </checkhead>\r\n <payments>\r\n  <payment id='1' sum='81'/>\r\n  <payment id='2' sum='27'/>\r\n </payments>\r\n <goods>\r\n  <good code='ЦУ000000031' name='Тестовый товар 1. WebCheck' quantity='9.00' price='9' sum='81' taxrate='1' uktzed='4002 20 00 00'/>\r\n  <good code='ЦУ000000029' name='Тестовый товар 2. WebCheck' quantity='3.00' price='9' sum='27' taxrate='1' uktzed='4002 20 00 00'/>\r\n </goods>\r\n</check>";
			if (WebCheck.All.WC.FiscalReceipt(strXML))
			{
				TC.SelectedIndex = 3;
			}
		}
	}

	private void Button4_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status && NewShift)
		{
			WebCheck.All.FN = "7000000512";
			if (WebCheck.All.WC.ReportZ("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>"))
			{
				TextBox4.Text = "Зміна закрита.";
				NewShift = false;
				Blokirovka();
				TC.TabPages[4].Enabled = true;
				TC.SelectedIndex = 4;
			}
		}
	}

	private void Button5_Click(object sender, EventArgs e)
	{
		if (WebCheck.All.status)
		{
			WebCheck.All.WC.ShowReports();
		}
	}

	private void ZeroPro()
	{
		((Control)TextBox6).Visible = true;
		((Control)Button6).Visible = true;
		((Control)Button7).Visible = true;
		((Control)Button8).Visible = true;
		((Control)PictureBox6).Visible = true;
		((Control)FNs).Enabled = false;
		((Control)Pro).Visible = false;
		TB.Text = WebCheck.All.FN;
		if (Operators.CompareString(WebCheck.All.FN, "7000000512", false) == 0)
		{
			if (!WebCheck.All.status)
			{
				WebCheck.All.FN = "7000000512";
				if (WebCheck.All.WC.Initialization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>"))
				{
					WebCheck.All.status = true;
				}
			}
		}
		else
		{
			WebCheck.All.WC.Finalization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
			WebCheck.All.FN = "7000000512";
			if (WebCheck.All.WC.Initialization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>"))
			{
				WebCheck.All.status = true;
			}
		}
	}

	private void TC_Selected(object sender, TabControlEventArgs e)
	{
		if (((Control)Pro).Visible && TC.SelectedIndex != 5)
		{
			ZeroPro();
		}
	}

	public void OpenURL(string wwwURL)
	{
		try
		{
			Process.Start(wwwURL);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void Button6_Click(object sender, EventArgs e)
	{
		OpenURL("http://www.webchek.com.ua");
	}

	private void Button7_Click(object sender, EventArgs e)
	{
		OpenURL("https://www.webchek.com.ua/podkluchenie1c/");
	}

	private void Button8_Click(object sender, EventArgs e)
	{
		OpenURL("https://docs.google.com/document/d/1q9Flnv81jTsX0cy3a5LRTjIa7ale7KU3hSv3SMekX_M/edit?usp=sharing");
	}

	private void ВидалитиДемонстраційнийНомерToolStripMenuItem_Click(object sender, EventArgs e)
	{
		WebCheck.All.FN = "7000000512";
		WebCheck.All.WC.Finalization("<InputParameters><Parameters FN='" + WebCheck.All.FN + "' OperatorID='1111111111'/></InputParameters>");
		WebCheck.All.WC.DeleteDemoBase();
		((Control)Pro).Visible = false;
		((Control)FNs).Enabled = false;
		NewNumber = false;
		Zapolnit();
		Blokirovka();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_004e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0058: Expected O, but got Unknown
		//IL_0059: Unknown result type (might be due to invalid IL or missing references)
		//IL_0063: Expected O, but got Unknown
		//IL_0064: Unknown result type (might be due to invalid IL or missing references)
		//IL_006e: Expected O, but got Unknown
		//IL_006f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0079: Expected O, but got Unknown
		//IL_007a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0084: Expected O, but got Unknown
		//IL_0085: Unknown result type (might be due to invalid IL or missing references)
		//IL_008f: Expected O, but got Unknown
		//IL_0090: Unknown result type (might be due to invalid IL or missing references)
		//IL_009a: Expected O, but got Unknown
		//IL_009b: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a5: Expected O, but got Unknown
		//IL_00a6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b0: Expected O, but got Unknown
		//IL_00b1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00bb: Expected O, but got Unknown
		//IL_00bc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c6: Expected O, but got Unknown
		//IL_00c7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d1: Expected O, but got Unknown
		//IL_00d2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00dc: Expected O, but got Unknown
		//IL_00dd: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e7: Expected O, but got Unknown
		//IL_00e8: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f2: Expected O, but got Unknown
		//IL_00f3: Unknown result type (might be due to invalid IL or missing references)
		//IL_00fd: Expected O, but got Unknown
		//IL_00fe: Unknown result type (might be due to invalid IL or missing references)
		//IL_0108: Expected O, but got Unknown
		//IL_0109: Unknown result type (might be due to invalid IL or missing references)
		//IL_0113: Expected O, but got Unknown
		//IL_0114: Unknown result type (might be due to invalid IL or missing references)
		//IL_011e: Expected O, but got Unknown
		//IL_011f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0129: Expected O, but got Unknown
		//IL_012a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0134: Expected O, but got Unknown
		//IL_0135: Unknown result type (might be due to invalid IL or missing references)
		//IL_013f: Expected O, but got Unknown
		//IL_0140: Unknown result type (might be due to invalid IL or missing references)
		//IL_014a: Expected O, but got Unknown
		//IL_014b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0155: Expected O, but got Unknown
		//IL_0156: Unknown result type (might be due to invalid IL or missing references)
		//IL_0160: Expected O, but got Unknown
		//IL_0161: Unknown result type (might be due to invalid IL or missing references)
		//IL_016b: Expected O, but got Unknown
		//IL_016c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0176: Expected O, but got Unknown
		//IL_0177: Unknown result type (might be due to invalid IL or missing references)
		//IL_0181: Expected O, but got Unknown
		//IL_0182: Unknown result type (might be due to invalid IL or missing references)
		//IL_018c: Expected O, but got Unknown
		//IL_018d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0197: Expected O, but got Unknown
		//IL_0198: Unknown result type (might be due to invalid IL or missing references)
		//IL_01a2: Expected O, but got Unknown
		//IL_01a3: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ad: Expected O, but got Unknown
		//IL_01ae: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b8: Expected O, but got Unknown
		//IL_01b9: Unknown result type (might be due to invalid IL or missing references)
		//IL_01c3: Expected O, but got Unknown
		//IL_01c4: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ce: Expected O, but got Unknown
		//IL_01cf: Unknown result type (might be due to invalid IL or missing references)
		//IL_01d9: Expected O, but got Unknown
		//IL_01da: Unknown result type (might be due to invalid IL or missing references)
		//IL_01e4: Expected O, but got Unknown
		//IL_01e5: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ef: Expected O, but got Unknown
		//IL_01f0: Unknown result type (might be due to invalid IL or missing references)
		//IL_01fa: Expected O, but got Unknown
		//IL_01fb: Unknown result type (might be due to invalid IL or missing references)
		//IL_0205: Expected O, but got Unknown
		//IL_0206: Unknown result type (might be due to invalid IL or missing references)
		//IL_0210: Expected O, but got Unknown
		//IL_02de: Unknown result type (might be due to invalid IL or missing references)
		//IL_02e8: Expected O, but got Unknown
		//IL_031c: Unknown result type (might be due to invalid IL or missing references)
		//IL_03c2: Unknown result type (might be due to invalid IL or missing references)
		//IL_0426: Unknown result type (might be due to invalid IL or missing references)
		//IL_0430: Expected O, but got Unknown
		//IL_044d: Unknown result type (might be due to invalid IL or missing references)
		//IL_04ca: Unknown result type (might be due to invalid IL or missing references)
		//IL_04d4: Expected O, but got Unknown
		//IL_04ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_08bf: Unknown result type (might be due to invalid IL or missing references)
		//IL_08c9: Expected O, but got Unknown
		//IL_098f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a84: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a8e: Expected O, but got Unknown
		//IL_0aa7: Unknown result type (might be due to invalid IL or missing references)
		//IL_0b17: Unknown result type (might be due to invalid IL or missing references)
		//IL_0b21: Expected O, but got Unknown
		//IL_0bf6: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ceb: Unknown result type (might be due to invalid IL or missing references)
		//IL_0cf5: Expected O, but got Unknown
		//IL_0d0e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0d7e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0d88: Expected O, but got Unknown
		//IL_0e5e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0f53: Unknown result type (might be due to invalid IL or missing references)
		//IL_0f5d: Expected O, but got Unknown
		//IL_0f76: Unknown result type (might be due to invalid IL or missing references)
		//IL_0fe6: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ff0: Expected O, but got Unknown
		//IL_10c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_11bb: Unknown result type (might be due to invalid IL or missing references)
		//IL_11c5: Expected O, but got Unknown
		//IL_11de: Unknown result type (might be due to invalid IL or missing references)
		//IL_124e: Unknown result type (might be due to invalid IL or missing references)
		//IL_1258: Expected O, but got Unknown
		//IL_132e: Unknown result type (might be due to invalid IL or missing references)
		//IL_1423: Unknown result type (might be due to invalid IL or missing references)
		//IL_142d: Expected O, but got Unknown
		//IL_1446: Unknown result type (might be due to invalid IL or missing references)
		//IL_14b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_14c0: Expected O, but got Unknown
		//IL_15d8: Unknown result type (might be due to invalid IL or missing references)
		//IL_163b: Unknown result type (might be due to invalid IL or missing references)
		//IL_1645: Expected O, but got Unknown
		//IL_16c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_16d0: Expected O, but got Unknown
		//IL_17e0: Unknown result type (might be due to invalid IL or missing references)
		//IL_17ea: Expected O, but got Unknown
		//IL_1803: Unknown result type (might be due to invalid IL or missing references)
		//IL_1873: Unknown result type (might be due to invalid IL or missing references)
		//IL_187d: Expected O, but got Unknown
		//IL_18fe: Unknown result type (might be due to invalid IL or missing references)
		//IL_1908: Expected O, but got Unknown
		//IL_1a40: Unknown result type (might be due to invalid IL or missing references)
		//IL_1a4a: Expected O, but got Unknown
		//IL_1a58: Unknown result type (might be due to invalid IL or missing references)
		components = new Container();
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form1));
		FNs = new ListBox();
		Pro = new Panel();
		StText1 = new TextBox();
		StText = new TextBox();
		ToolTip1 = new ToolTip(components);
		MenuStrip1 = new MenuStrip();
		МенюToolStripMenuItem = new ToolStripMenuItem();
		СоздатьНовуюPROToolStripMenuItem = new ToolStripMenuItem();
		ПереглядЗмінІЧеківToolStripMenuItem = new ToolStripMenuItem();
		ОператориToolStripMenuItem = new ToolStripMenuItem();
		ДрукЧеківToolStripMenuItem = new ToolStripMenuItem();
		ЛицензииToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem1 = new ToolStripSeparator();
		ЗакритиToolStripMenuItem = new ToolStripMenuItem();
		ОПрограммеToolStripMenuItem = new ToolStripMenuItem();
		TC = new TabControl();
		TabPage1 = new TabPage();
		PictureBox1 = new PictureBox();
		TextBox1 = new TextBox();
		Button1 = new Button();
		TabPage2 = new TabPage();
		PictureBox2 = new PictureBox();
		TextBox2 = new TextBox();
		Button2 = new Button();
		TabPage3 = new TabPage();
		PictureBox3 = new PictureBox();
		TextBox3 = new TextBox();
		Button3 = new Button();
		TabPage4 = new TabPage();
		PictureBox4 = new PictureBox();
		TextBox4 = new TextBox();
		Button4 = new Button();
		TabPage5 = new TabPage();
		PictureBox5 = new PictureBox();
		TextBox5 = new TextBox();
		Button5 = new Button();
		TabPage6 = new TabPage();
		Button8 = new Button();
		Button7 = new Button();
		PictureBox6 = new PictureBox();
		TextBox6 = new TextBox();
		Button6 = new Button();
		TB = new TextBox();
		ВидалитиДемонстраційнийНомерToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem2 = new ToolStripSeparator();
		((Control)Pro).SuspendLayout();
		((Control)MenuStrip1).SuspendLayout();
		((Control)TC).SuspendLayout();
		((Control)TabPage1).SuspendLayout();
		((ISupportInitialize)PictureBox1).BeginInit();
		((Control)TabPage2).SuspendLayout();
		((ISupportInitialize)PictureBox2).BeginInit();
		((Control)TabPage3).SuspendLayout();
		((ISupportInitialize)PictureBox3).BeginInit();
		((Control)TabPage4).SuspendLayout();
		((ISupportInitialize)PictureBox4).BeginInit();
		((Control)TabPage5).SuspendLayout();
		((ISupportInitialize)PictureBox5).BeginInit();
		((Control)TabPage6).SuspendLayout();
		((ISupportInitialize)PictureBox6).BeginInit();
		((Control)this).SuspendLayout();
		((Control)FNs).Anchor = (AnchorStyles)7;
		FNs.Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)FNs).FormattingEnabled = true;
		FNs.ItemHeight = 25;
		((Control)FNs).Location = new Point(13, 89);
		((Control)FNs).Margin = new Padding(4);
		((Control)FNs).Name = "FNs";
		((Control)FNs).Size = new Size(173, 504);
		((Control)FNs).TabIndex = 0;
		((Control)Pro).Anchor = (AnchorStyles)13;
		Pro.BorderStyle = (BorderStyle)1;
		((Control)Pro).Controls.Add((Control)(object)StText1);
		((Control)Pro).Controls.Add((Control)(object)StText);
		((Control)Pro).Location = new Point(912, 286);
		((Control)Pro).Margin = new Padding(4);
		((Control)Pro).Name = "Pro";
		((Control)Pro).Size = new Size(1074, 476);
		((Control)Pro).TabIndex = 5;
		((Control)StText1).Anchor = (AnchorStyles)13;
		((Control)StText1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)StText1).Location = new Point(4, 129);
		((Control)StText1).Margin = new Padding(4);
		StText1.Multiline = true;
		((Control)StText1).Name = "StText1";
		((Control)StText1).Size = new Size(1064, 341);
		((Control)StText1).TabIndex = 10;
		StText1.TextAlign = (HorizontalAlignment)2;
		((Control)StText).Anchor = (AnchorStyles)13;
		((Control)StText).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)StText).Location = new Point(4, 4);
		((Control)StText).Margin = new Padding(4);
		StText.Multiline = true;
		((Control)StText).Name = "StText";
		((Control)StText).Size = new Size(1064, 117);
		((Control)StText).TabIndex = 9;
		StText.TextAlign = (HorizontalAlignment)2;
		((ToolStrip)MenuStrip1).ImageScalingSize = new Size(20, 20);
		((ToolStrip)MenuStrip1).Items.AddRange((ToolStripItem[])(object)new ToolStripItem[2]
		{
			(ToolStripItem)МенюToolStripMenuItem,
			(ToolStripItem)ОПрограммеToolStripMenuItem
		});
		((Control)MenuStrip1).Location = new Point(0, 0);
		((Control)MenuStrip1).Name = "MenuStrip1";
		((Control)MenuStrip1).Size = new Size(1331, 28);
		((Control)MenuStrip1).TabIndex = 7;
		((Control)MenuStrip1).Text = "MenuStrip1";
		((ToolStripDropDownItem)МенюToolStripMenuItem).DropDownItems.AddRange((ToolStripItem[])(object)new ToolStripItem[9]
		{
			(ToolStripItem)СоздатьНовуюPROToolStripMenuItem,
			(ToolStripItem)ПереглядЗмінІЧеківToolStripMenuItem,
			(ToolStripItem)ОператориToolStripMenuItem,
			(ToolStripItem)ДрукЧеківToolStripMenuItem,
			(ToolStripItem)ЛицензииToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem2,
			(ToolStripItem)ВидалитиДемонстраційнийНомерToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem1,
			(ToolStripItem)ЗакритиToolStripMenuItem
		});
		((ToolStripItem)МенюToolStripMenuItem).Name = "МенюToolStripMenuItem";
		((ToolStripItem)МенюToolStripMenuItem).Size = new Size(65, 24);
		((ToolStripItem)МенюToolStripMenuItem).Text = "Меню";
		((ToolStripItem)СоздатьНовуюPROToolStripMenuItem).Name = "СоздатьНовуюPROToolStripMenuItem";
		((ToolStripItem)СоздатьНовуюPROToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)СоздатьНовуюPROToolStripMenuItem).Text = "Створити демо ПРРО...";
		((ToolStripItem)ПереглядЗмінІЧеківToolStripMenuItem).Name = "ПереглядЗмінІЧеківToolStripMenuItem";
		((ToolStripItem)ПереглядЗмінІЧеківToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ПереглядЗмінІЧеківToolStripMenuItem).Text = "Перегляд змін та чеків...";
		((ToolStripItem)ОператориToolStripMenuItem).Name = "ОператориToolStripMenuItem";
		((ToolStripItem)ОператориToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ОператориToolStripMenuItem).Text = "Перелік операторів...";
		((ToolStripItem)ДрукЧеківToolStripMenuItem).Name = "ДрукЧеківToolStripMenuItem";
		((ToolStripItem)ДрукЧеківToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ДрукЧеківToolStripMenuItem).Text = "Друк чеків...";
		((ToolStripItem)ЛицензииToolStripMenuItem).Name = "ЛицензииToolStripMenuItem";
		((ToolStripItem)ЛицензииToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ЛицензииToolStripMenuItem).Text = "Ліцензії...";
		((ToolStripItem)ToolStripMenuItem1).Name = "ToolStripMenuItem1";
		((ToolStripItem)ToolStripMenuItem1).Size = new Size(333, 6);
		((ToolStripItem)ЗакритиToolStripMenuItem).Name = "ЗакритиToolStripMenuItem";
		((ToolStripItem)ЗакритиToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ЗакритиToolStripMenuItem).Text = "Закрити";
		((ToolStripItem)ОПрограммеToolStripMenuItem).Name = "ОПрограммеToolStripMenuItem";
		((ToolStripItem)ОПрограммеToolStripMenuItem).Size = new Size(133, 24);
		((ToolStripItem)ОПрограммеToolStripMenuItem).Text = "Про програму...";
		((Control)TC).Anchor = (AnchorStyles)15;
		((Control)TC).Controls.Add((Control)(object)TabPage1);
		((Control)TC).Controls.Add((Control)(object)TabPage2);
		((Control)TC).Controls.Add((Control)(object)TabPage3);
		((Control)TC).Controls.Add((Control)(object)TabPage4);
		((Control)TC).Controls.Add((Control)(object)TabPage5);
		((Control)TC).Controls.Add((Control)(object)TabPage6);
		((Control)TC).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TC).Location = new Point(196, 39);
		((Control)TC).Name = "TC";
		TC.SelectedIndex = 0;
		((Control)TC).Size = new Size(1123, 551);
		((Control)TC).TabIndex = 9;
		((Control)TabPage1).Controls.Add((Control)(object)PictureBox1);
		((Control)TabPage1).Controls.Add((Control)(object)TextBox1);
		((Control)TabPage1).Controls.Add((Control)(object)Button1);
		TabPage1.Location = new Point(4, 29);
		((Control)TabPage1).Name = "TabPage1";
		((Control)TabPage1).Padding = new Padding(3);
		((Control)TabPage1).Size = new Size(1115, 518);
		TabPage1.TabIndex = 6;
		TabPage1.Text = "Створення бази";
		TabPage1.UseVisualStyleBackColor = true;
		PictureBox1.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox1).Location = new Point(222, 314);
		((Control)PictureBox1).Name = "PictureBox1";
		((Control)PictureBox1).Size = new Size(500, 140);
		PictureBox1.SizeMode = (PictureBoxSizeMode)2;
		PictureBox1.TabIndex = 11;
		PictureBox1.TabStop = false;
		((Control)TextBox1).Anchor = (AnchorStyles)13;
		((Control)TextBox1).Enabled = false;
		((Control)TextBox1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox1).Location = new Point(7, 7);
		((Control)TextBox1).Margin = new Padding(4);
		TextBox1.Multiline = true;
		((Control)TextBox1).Name = "TextBox1";
		((Control)TextBox1).Size = new Size(1101, 203);
		((Control)TextBox1).TabIndex = 10;
		TextBox1.TextAlign = (HorizontalAlignment)2;
		((Control)Button1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button1).Location = new Point(401, 228);
		((Control)Button1).Name = "Button1";
		((Control)Button1).Size = new Size(305, 39);
		((Control)Button1).TabIndex = 0;
		((ButtonBase)Button1).Text = "Створити тестову базу";
		((ButtonBase)Button1).UseVisualStyleBackColor = true;
		((Control)TabPage2).Controls.Add((Control)(object)PictureBox2);
		((Control)TabPage2).Controls.Add((Control)(object)TextBox2);
		((Control)TabPage2).Controls.Add((Control)(object)Button2);
		TabPage2.Location = new Point(4, 29);
		((Control)TabPage2).Name = "TabPage2";
		((Control)TabPage2).Padding = new Padding(3);
		((Control)TabPage2).Size = new Size(1115, 518);
		TabPage2.TabIndex = 1;
		TabPage2.Text = "Відкриття зміни";
		TabPage2.UseVisualStyleBackColor = true;
		PictureBox2.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox2).Location = new Point(222, 314);
		((Control)PictureBox2).Name = "PictureBox2";
		((Control)PictureBox2).Size = new Size(500, 140);
		PictureBox2.SizeMode = (PictureBoxSizeMode)2;
		PictureBox2.TabIndex = 14;
		PictureBox2.TabStop = false;
		((Control)TextBox2).Anchor = (AnchorStyles)13;
		((Control)TextBox2).Enabled = false;
		((Control)TextBox2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox2).Location = new Point(7, 7);
		((Control)TextBox2).Margin = new Padding(4);
		TextBox2.Multiline = true;
		((Control)TextBox2).Name = "TextBox2";
		((Control)TextBox2).Size = new Size(1101, 203);
		((Control)TextBox2).TabIndex = 13;
		TextBox2.TextAlign = (HorizontalAlignment)2;
		((Control)Button2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button2).Location = new Point(401, 228);
		((Control)Button2).Name = "Button2";
		((Control)Button2).Size = new Size(305, 39);
		((Control)Button2).TabIndex = 12;
		((ButtonBase)Button2).Text = "Відкрити зміну";
		((ButtonBase)Button2).UseVisualStyleBackColor = true;
		((Control)TabPage3).Controls.Add((Control)(object)PictureBox3);
		((Control)TabPage3).Controls.Add((Control)(object)TextBox3);
		((Control)TabPage3).Controls.Add((Control)(object)Button3);
		TabPage3.Location = new Point(4, 29);
		((Control)TabPage3).Name = "TabPage3";
		((Control)TabPage3).Padding = new Padding(3);
		((Control)TabPage3).Size = new Size(1115, 518);
		TabPage3.TabIndex = 2;
		TabPage3.Text = "Фіскалізація чеків";
		TabPage3.UseVisualStyleBackColor = true;
		PictureBox3.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox3).Location = new Point(222, 314);
		((Control)PictureBox3).Name = "PictureBox3";
		((Control)PictureBox3).Size = new Size(500, 140);
		PictureBox3.SizeMode = (PictureBoxSizeMode)2;
		PictureBox3.TabIndex = 14;
		PictureBox3.TabStop = false;
		((Control)TextBox3).Anchor = (AnchorStyles)13;
		((Control)TextBox3).Enabled = false;
		((Control)TextBox3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox3).Location = new Point(7, 7);
		((Control)TextBox3).Margin = new Padding(4);
		TextBox3.Multiline = true;
		((Control)TextBox3).Name = "TextBox3";
		((Control)TextBox3).Size = new Size(1101, 203);
		((Control)TextBox3).TabIndex = 13;
		TextBox3.TextAlign = (HorizontalAlignment)2;
		((Control)Button3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button3).Location = new Point(401, 228);
		((Control)Button3).Name = "Button3";
		((Control)Button3).Size = new Size(305, 39);
		((Control)Button3).TabIndex = 12;
		((ButtonBase)Button3).Text = "Фіскальний чек";
		((ButtonBase)Button3).UseVisualStyleBackColor = true;
		((Control)TabPage4).Controls.Add((Control)(object)PictureBox4);
		((Control)TabPage4).Controls.Add((Control)(object)TextBox4);
		((Control)TabPage4).Controls.Add((Control)(object)Button4);
		TabPage4.Location = new Point(4, 29);
		((Control)TabPage4).Name = "TabPage4";
		((Control)TabPage4).Padding = new Padding(3);
		((Control)TabPage4).Size = new Size(1115, 518);
		TabPage4.TabIndex = 3;
		TabPage4.Text = "Закриття зміни (Z звіт)";
		TabPage4.UseVisualStyleBackColor = true;
		PictureBox4.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox4).Location = new Point(222, 314);
		((Control)PictureBox4).Name = "PictureBox4";
		((Control)PictureBox4).Size = new Size(500, 140);
		PictureBox4.SizeMode = (PictureBoxSizeMode)2;
		PictureBox4.TabIndex = 14;
		PictureBox4.TabStop = false;
		((Control)TextBox4).Anchor = (AnchorStyles)13;
		((Control)TextBox4).Enabled = false;
		((Control)TextBox4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox4).Location = new Point(7, 7);
		((Control)TextBox4).Margin = new Padding(4);
		TextBox4.Multiline = true;
		((Control)TextBox4).Name = "TextBox4";
		((Control)TextBox4).Size = new Size(1101, 203);
		((Control)TextBox4).TabIndex = 13;
		TextBox4.TextAlign = (HorizontalAlignment)2;
		((Control)Button4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button4).Location = new Point(401, 228);
		((Control)Button4).Name = "Button4";
		((Control)Button4).Size = new Size(305, 39);
		((Control)Button4).TabIndex = 12;
		((ButtonBase)Button4).Text = "Закрити зміну (Z звіт)";
		((ButtonBase)Button4).UseVisualStyleBackColor = true;
		((Control)TabPage5).Controls.Add((Control)(object)PictureBox5);
		((Control)TabPage5).Controls.Add((Control)(object)TextBox5);
		((Control)TabPage5).Controls.Add((Control)(object)Button5);
		TabPage5.Location = new Point(4, 29);
		((Control)TabPage5).Name = "TabPage5";
		((Control)TabPage5).Padding = new Padding(3);
		((Control)TabPage5).Size = new Size(1115, 518);
		TabPage5.TabIndex = 4;
		TabPage5.Text = "Перегляд змін і чеків";
		TabPage5.UseVisualStyleBackColor = true;
		PictureBox5.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox5).Location = new Point(222, 314);
		((Control)PictureBox5).Name = "PictureBox5";
		((Control)PictureBox5).Size = new Size(500, 140);
		PictureBox5.SizeMode = (PictureBoxSizeMode)2;
		PictureBox5.TabIndex = 14;
		PictureBox5.TabStop = false;
		((Control)TextBox5).Anchor = (AnchorStyles)13;
		((Control)TextBox5).Enabled = false;
		((Control)TextBox5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox5).Location = new Point(7, 7);
		((Control)TextBox5).Margin = new Padding(4);
		TextBox5.Multiline = true;
		((Control)TextBox5).Name = "TextBox5";
		((Control)TextBox5).Size = new Size(1101, 203);
		((Control)TextBox5).TabIndex = 13;
		TextBox5.TextAlign = (HorizontalAlignment)2;
		((Control)Button5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button5).Location = new Point(401, 228);
		((Control)Button5).Name = "Button5";
		((Control)Button5).Size = new Size(305, 39);
		((Control)Button5).TabIndex = 12;
		((ButtonBase)Button5).Text = "Зміни та чеки";
		((ButtonBase)Button5).UseVisualStyleBackColor = true;
		((Control)TabPage6).Controls.Add((Control)(object)Button8);
		((Control)TabPage6).Controls.Add((Control)(object)Button7);
		((Control)TabPage6).Controls.Add((Control)(object)Pro);
		((Control)TabPage6).Controls.Add((Control)(object)PictureBox6);
		((Control)TabPage6).Controls.Add((Control)(object)TextBox6);
		((Control)TabPage6).Controls.Add((Control)(object)Button6);
		TabPage6.Location = new Point(4, 29);
		((Control)TabPage6).Name = "TabPage6";
		((Control)TabPage6).Padding = new Padding(3);
		((Control)TabPage6).Size = new Size(1115, 518);
		TabPage6.TabIndex = 5;
		TabPage6.Text = "Корисна інформація";
		TabPage6.UseVisualStyleBackColor = true;
		((Control)Button8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button8).Location = new Point(747, 228);
		((Control)Button8).Name = "Button8";
		((Control)Button8).Size = new Size(305, 39);
		((Control)Button8).TabIndex = 19;
		((ButtonBase)Button8).Text = "Документація";
		((ButtonBase)Button8).UseVisualStyleBackColor = true;
		((Control)Button7).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button7).Location = new Point(53, 228);
		((Control)Button7).Name = "Button7";
		((Control)Button7).Size = new Size(305, 39);
		((Control)Button7).TabIndex = 18;
		((ButtonBase)Button7).Text = "ПРРО до 1С Підприемство";
		((ButtonBase)Button7).UseVisualStyleBackColor = true;
		PictureBox6.Image = (Image)(object)WebCheck.My.Resources.Resources.logochek;
		((Control)PictureBox6).Location = new Point(222, 314);
		((Control)PictureBox6).Name = "PictureBox6";
		((Control)PictureBox6).Size = new Size(500, 140);
		PictureBox6.SizeMode = (PictureBoxSizeMode)2;
		PictureBox6.TabIndex = 17;
		PictureBox6.TabStop = false;
		((Control)TextBox6).Anchor = (AnchorStyles)13;
		((Control)TextBox6).Enabled = false;
		((Control)TextBox6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox6).Location = new Point(7, 7);
		((Control)TextBox6).Margin = new Padding(4);
		TextBox6.Multiline = true;
		((Control)TextBox6).Name = "TextBox6";
		((Control)TextBox6).Size = new Size(1101, 203);
		((Control)TextBox6).TabIndex = 16;
		TextBox6.TextAlign = (HorizontalAlignment)2;
		((Control)Button6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button6).Location = new Point(401, 228);
		((Control)Button6).Name = "Button6";
		((Control)Button6).Size = new Size(305, 39);
		((Control)Button6).TabIndex = 15;
		((ButtonBase)Button6).Text = "Відвідати сайт";
		((ButtonBase)Button6).UseVisualStyleBackColor = true;
		((Control)TB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TB).Location = new Point(12, 39);
		((Control)TB).Name = "TB";
		((Control)TB).Size = new Size(173, 30);
		((Control)TB).TabIndex = 0;
		TB.TextAlign = (HorizontalAlignment)2;
		((ToolStripItem)ВидалитиДемонстраційнийНомерToolStripMenuItem).Name = "ВидалитиДемонстраційнийНомерToolStripMenuItem";
		((ToolStripItem)ВидалитиДемонстраційнийНомерToolStripMenuItem).Size = new Size(336, 26);
		((ToolStripItem)ВидалитиДемонстраційнийНомерToolStripMenuItem).Text = "Видалити демонстраційний номер";
		((ToolStripItem)ToolStripMenuItem2).Name = "ToolStripMenuItem2";
		((ToolStripItem)ToolStripMenuItem2).Size = new Size(333, 6);
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1331, 603);
		((Control)this).Controls.Add((Control)(object)TB);
		((Control)this).Controls.Add((Control)(object)TC);
		((Control)this).Controls.Add((Control)(object)FNs);
		((Control)this).Controls.Add((Control)(object)MenuStrip1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MainMenuStrip = MenuStrip1;
		((Form)this).Margin = new Padding(4);
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "Form1";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Знайомство з ПРРО";
		((Control)Pro).ResumeLayout(false);
		((Control)Pro).PerformLayout();
		((Control)MenuStrip1).ResumeLayout(false);
		((Control)MenuStrip1).PerformLayout();
		((Control)TC).ResumeLayout(false);
		((Control)TabPage1).ResumeLayout(false);
		((Control)TabPage1).PerformLayout();
		((ISupportInitialize)PictureBox1).EndInit();
		((Control)TabPage2).ResumeLayout(false);
		((Control)TabPage2).PerformLayout();
		((ISupportInitialize)PictureBox2).EndInit();
		((Control)TabPage3).ResumeLayout(false);
		((Control)TabPage3).PerformLayout();
		((ISupportInitialize)PictureBox3).EndInit();
		((Control)TabPage4).ResumeLayout(false);
		((Control)TabPage4).PerformLayout();
		((ISupportInitialize)PictureBox4).EndInit();
		((Control)TabPage5).ResumeLayout(false);
		((Control)TabPage5).PerformLayout();
		((ISupportInitialize)PictureBox5).EndInit();
		((Control)TabPage6).ResumeLayout(false);
		((Control)TabPage6).PerformLayout();
		((ISupportInitialize)PictureBox6).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}
}
