using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormAccountant : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиПідприємствоToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиПідприємствоToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("CBorg")]
	private ComboBox _CBorg;

	[CompilerGenerated]
	[AccessedThroughProperty("LBfn")]
	private CheckedListBox _LBfn;

	[CompilerGenerated]
	[AccessedThroughProperty("UpdateB")]
	private Button _UpdateB;

	[CompilerGenerated]
	[AccessedThroughProperty("ChecksB")]
	private Button _ChecksB;

	[CompilerGenerated]
	[AccessedThroughProperty("ReportsB")]
	private Button _ReportsB;

	[CompilerGenerated]
	[AccessedThroughProperty("SearchB")]
	private Button _SearchB;

	[CompilerGenerated]
	[AccessedThroughProperty("SeaT")]
	private TextBox _SeaT;

	[CompilerGenerated]
	[AccessedThroughProperty("NoSB")]
	private Button _NoSB;

	[CompilerGenerated]
	[AccessedThroughProperty("ОновитиДаніПідприємстваToolStripMenuItem")]
	private ToolStripMenuItem _ОновитиДаніПідприємстваToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЕкспортЧеківЗаПеріодToolStripMenuItem")]
	private ToolStripMenuItem _ЕкспортЧеківЗаПеріодToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ДоступніЛіцензіїToolStripMenuItem")]
	private ToolStripMenuItem _ДоступніЛіцензіїToolStripMenuItem;

	private AccountantОffice AO;

	private bool fullD;

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

	internal virtual ToolStripMenuItem ДодатиПідприємствоToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиПідприємствоToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ДодатиПідприємствоToolStripMenuItem_Click;
			ToolStripMenuItem додатиПідприємствоToolStripMenuItem = _ДодатиПідприємствоToolStripMenuItem;
			if (додатиПідприємствоToolStripMenuItem != null)
			{
				((ToolStripItem)додатиПідприємствоToolStripMenuItem).Click -= eventHandler;
			}
			_ДодатиПідприємствоToolStripMenuItem = value;
			додатиПідприємствоToolStripMenuItem = _ДодатиПідприємствоToolStripMenuItem;
			if (додатиПідприємствоToolStripMenuItem != null)
			{
				((ToolStripItem)додатиПідприємствоToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ComboBox CBorg
	{
		[CompilerGenerated]
		get
		{
			return _CBorg;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CBorg_SelectedIndexChanged;
			ComboBox cBorg = _CBorg;
			if (cBorg != null)
			{
				cBorg.SelectedIndexChanged -= eventHandler;
			}
			_CBorg = value;
			cBorg = _CBorg;
			if (cBorg != null)
			{
				cBorg.SelectedIndexChanged += eventHandler;
			}
		}
	}

	internal virtual CheckedListBox LBfn
	{
		[CompilerGenerated]
		get
		{
			return _LBfn;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = LBfn_SelectedIndexChanged;
			CheckedListBox lBfn = _LBfn;
			if (lBfn != null)
			{
				((ListBox)lBfn).SelectedIndexChanged -= eventHandler;
			}
			_LBfn = value;
			lBfn = _LBfn;
			if (lBfn != null)
			{
				((ListBox)lBfn).SelectedIndexChanged += eventHandler;
			}
		}
	}

	internal virtual Button UpdateB
	{
		[CompilerGenerated]
		get
		{
			return _UpdateB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = UpdateB_Click;
			Button updateB = _UpdateB;
			if (updateB != null)
			{
				((Control)updateB).Click -= eventHandler;
			}
			_UpdateB = value;
			updateB = _UpdateB;
			if (updateB != null)
			{
				((Control)updateB).Click += eventHandler;
			}
		}
	}

	internal virtual Button ChecksB
	{
		[CompilerGenerated]
		get
		{
			return _ChecksB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ChecksB_Click;
			Button checksB = _ChecksB;
			if (checksB != null)
			{
				((Control)checksB).Click -= eventHandler;
			}
			_ChecksB = value;
			checksB = _ChecksB;
			if (checksB != null)
			{
				((Control)checksB).Click += eventHandler;
			}
		}
	}

	internal virtual Button ReportsB
	{
		[CompilerGenerated]
		get
		{
			return _ReportsB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ReportsB_Click;
			Button reportsB = _ReportsB;
			if (reportsB != null)
			{
				((Control)reportsB).Click -= eventHandler;
			}
			_ReportsB = value;
			reportsB = _ReportsB;
			if (reportsB != null)
			{
				((Control)reportsB).Click += eventHandler;
			}
		}
	}

	internal virtual Button SearchB
	{
		[CompilerGenerated]
		get
		{
			return _SearchB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SearchB_Click;
			Button searchB = _SearchB;
			if (searchB != null)
			{
				((Control)searchB).Click -= eventHandler;
			}
			_SearchB = value;
			searchB = _SearchB;
			if (searchB != null)
			{
				((Control)searchB).Click += eventHandler;
			}
		}
	}

	internal virtual TextBox SeaT
	{
		[CompilerGenerated]
		get
		{
			return _SeaT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			MouseEventHandler val = new MouseEventHandler(SeaT_MouseDoubleClick);
			TextBox seaT = _SeaT;
			if (seaT != null)
			{
				((Control)seaT).MouseDoubleClick -= val;
			}
			_SeaT = value;
			seaT = _SeaT;
			if (seaT != null)
			{
				((Control)seaT).MouseDoubleClick += val;
			}
		}
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("IndiT")]
	internal virtual TextBox IndiT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DateUpdadeT")]
	internal virtual TextBox DateUpdadeT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoSB
	{
		[CompilerGenerated]
		get
		{
			return _NoSB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoSB_Click;
			Button noSB = _NoSB;
			if (noSB != null)
			{
				((Control)noSB).Click -= eventHandler;
			}
			_NoSB = value;
			noSB = _NoSB;
			if (noSB != null)
			{
				((Control)noSB).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ОновитиДаніПідприємстваToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОновитиДаніПідприємстваToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ОновитиДаніПідприємстваToolStripMenuItem_Click;
			ToolStripMenuItem оновитиДаніПідприємстваToolStripMenuItem = _ОновитиДаніПідприємстваToolStripMenuItem;
			if (оновитиДаніПідприємстваToolStripMenuItem != null)
			{
				((ToolStripItem)оновитиДаніПідприємстваToolStripMenuItem).Click -= eventHandler;
			}
			_ОновитиДаніПідприємстваToolStripMenuItem = value;
			оновитиДаніПідприємстваToolStripMenuItem = _ОновитиДаніПідприємстваToolStripMenuItem;
			if (оновитиДаніПідприємстваToolStripMenuItem != null)
			{
				((ToolStripItem)оновитиДаніПідприємстваToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЕкспортЧеківЗаПеріодToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЕкспортЧеківЗаПеріодToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ЕкспортЧеківЗаПеріодToolStripMenuItem_Click;
			ToolStripMenuItem експортЧеківЗаПеріодToolStripMenuItem = _ЕкспортЧеківЗаПеріодToolStripMenuItem;
			if (експортЧеківЗаПеріодToolStripMenuItem != null)
			{
				((ToolStripItem)експортЧеківЗаПеріодToolStripMenuItem).Click -= eventHandler;
			}
			_ЕкспортЧеківЗаПеріодToolStripMenuItem = value;
			експортЧеківЗаПеріодToolStripMenuItem = _ЕкспортЧеківЗаПеріодToolStripMenuItem;
			if (експортЧеківЗаПеріодToolStripMenuItem != null)
			{
				((ToolStripItem)експортЧеківЗаПеріодToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДоступніЛіцензіїToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДоступніЛіцензіїToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ДоступніЛіцензіїToolStripMenuItem_Click;
			ToolStripMenuItem доступніЛіцензіїToolStripMenuItem = _ДоступніЛіцензіїToolStripMenuItem;
			if (доступніЛіцензіїToolStripMenuItem != null)
			{
				((ToolStripItem)доступніЛіцензіїToolStripMenuItem).Click -= eventHandler;
			}
			_ДоступніЛіцензіїToolStripMenuItem = value;
			доступніЛіцензіїToolStripMenuItem = _ДоступніЛіцензіїToolStripMenuItem;
			if (доступніЛіцензіїToolStripMenuItem != null)
			{
				((ToolStripItem)доступніЛіцензіїToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	public FormAccountant()
	{
		((Form)this).Load += FormAccountant_Load;
		AO = new AccountantОffice();
		fullD = false;
		InitializeComponent();
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
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_00b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Expected O, but got Unknown
		//IL_00c1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cb: Expected O, but got Unknown
		//IL_00cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d6: Expected O, but got Unknown
		//IL_00d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e1: Expected O, but got Unknown
		//IL_02e8: Unknown result type (might be due to invalid IL or missing references)
		//IL_02f2: Expected O, but got Unknown
		//IL_036c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0376: Expected O, but got Unknown
		//IL_03f2: Unknown result type (might be due to invalid IL or missing references)
		//IL_03fc: Expected O, but got Unknown
		//IL_0476: Unknown result type (might be due to invalid IL or missing references)
		//IL_0480: Expected O, but got Unknown
		//IL_04fd: Unknown result type (might be due to invalid IL or missing references)
		//IL_0507: Expected O, but got Unknown
		//IL_0584: Unknown result type (might be due to invalid IL or missing references)
		//IL_058e: Expected O, but got Unknown
		//IL_060b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0615: Expected O, but got Unknown
		//IL_068e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0698: Expected O, but got Unknown
		//IL_0710: Unknown result type (might be due to invalid IL or missing references)
		//IL_071a: Expected O, but got Unknown
		//IL_07ad: Unknown result type (might be due to invalid IL or missing references)
		//IL_07b7: Expected O, but got Unknown
		//IL_083d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0847: Expected O, but got Unknown
		//IL_0a11: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a1b: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormAccountant));
		MenuStrip1 = new MenuStrip();
		МенюToolStripMenuItem = new ToolStripMenuItem();
		ДодатиПідприємствоToolStripMenuItem = new ToolStripMenuItem();
		ОновитиДаніПідприємстваToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem1 = new ToolStripSeparator();
		ЕкспортЧеківЗаПеріодToolStripMenuItem = new ToolStripMenuItem();
		CBorg = new ComboBox();
		LBfn = new CheckedListBox();
		UpdateB = new Button();
		ChecksB = new Button();
		ReportsB = new Button();
		SearchB = new Button();
		SeaT = new TextBox();
		Label2 = new Label();
		IndiT = new TextBox();
		DateUpdadeT = new TextBox();
		NoSB = new Button();
		ToolStripMenuItem2 = new ToolStripSeparator();
		ДоступніЛіцензіїToolStripMenuItem = new ToolStripMenuItem();
		((Control)MenuStrip1).SuspendLayout();
		((Control)this).SuspendLayout();
		((ToolStrip)MenuStrip1).ImageScalingSize = new Size(20, 20);
		((ToolStrip)MenuStrip1).Items.AddRange((ToolStripItem[])(object)new ToolStripItem[1] { (ToolStripItem)МенюToolStripMenuItem });
		((Control)MenuStrip1).Location = new Point(0, 0);
		((Control)MenuStrip1).Name = "MenuStrip1";
		((Control)MenuStrip1).Size = new Size(1129, 28);
		((Control)MenuStrip1).TabIndex = 0;
		((Control)MenuStrip1).Text = "MenuStrip1";
		((ToolStripDropDownItem)МенюToolStripMenuItem).DropDownItems.AddRange((ToolStripItem[])(object)new ToolStripItem[6]
		{
			(ToolStripItem)ДодатиПідприємствоToolStripMenuItem,
			(ToolStripItem)ОновитиДаніПідприємстваToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem2,
			(ToolStripItem)ДоступніЛіцензіїToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem1,
			(ToolStripItem)ЕкспортЧеківЗаПеріодToolStripMenuItem
		});
		((ToolStripItem)МенюToolStripMenuItem).Name = "МенюToolStripMenuItem";
		((ToolStripItem)МенюToolStripMenuItem).Size = new Size(65, 24);
		((ToolStripItem)МенюToolStripMenuItem).Text = "Меню";
		((ToolStripItem)ДодатиПідприємствоToolStripMenuItem).Name = "ДодатиПідприємствоToolStripMenuItem";
		((ToolStripItem)ДодатиПідприємствоToolStripMenuItem).Size = new Size(294, 26);
		((ToolStripItem)ДодатиПідприємствоToolStripMenuItem).Text = "Додати підприємство...";
		((ToolStripItem)ОновитиДаніПідприємстваToolStripMenuItem).Name = "ОновитиДаніПідприємстваToolStripMenuItem";
		((ToolStripItem)ОновитиДаніПідприємстваToolStripMenuItem).Size = new Size(294, 26);
		((ToolStripItem)ОновитиДаніПідприємстваToolStripMenuItem).Text = "Оновити дані підприємства...";
		((ToolStripItem)ToolStripMenuItem1).Name = "ToolStripMenuItem1";
		((ToolStripItem)ToolStripMenuItem1).Size = new Size(291, 6);
		((ToolStripItem)ЕкспортЧеківЗаПеріодToolStripMenuItem).Name = "ЕкспортЧеківЗаПеріодToolStripMenuItem";
		((ToolStripItem)ЕкспортЧеківЗаПеріодToolStripMenuItem).Size = new Size(294, 26);
		((ToolStripItem)ЕкспортЧеківЗаПеріодToolStripMenuItem).Text = "Експорт чеків за період...";
		CBorg.DropDownStyle = (ComboBoxStyle)2;
		((Control)CBorg).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)CBorg).FormattingEnabled = true;
		((Control)CBorg).Location = new Point(129, 41);
		((Control)CBorg).Name = "CBorg";
		((Control)CBorg).Size = new Size(933, 33);
		((Control)CBorg).TabIndex = 1;
		((Control)LBfn).Anchor = (AnchorStyles)15;
		((ListBox)LBfn).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)LBfn).FormattingEnabled = true;
		((ListBox)LBfn).HorizontalScrollbar = true;
		((Control)LBfn).Location = new Point(12, 140);
		((Control)LBfn).Name = "LBfn";
		((Control)LBfn).Size = new Size(1105, 444);
		((Control)LBfn).TabIndex = 2;
		((Control)UpdateB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)UpdateB).Location = new Point(12, 83);
		((Control)UpdateB).Name = "UpdateB";
		((Control)UpdateB).Size = new Size(137, 47);
		((Control)UpdateB).TabIndex = 3;
		((ButtonBase)UpdateB).Text = "Оновити";
		((ButtonBase)UpdateB).UseVisualStyleBackColor = true;
		((Control)ChecksB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ChecksB).Location = new Point(335, 83);
		((Control)ChecksB).Name = "ChecksB";
		((Control)ChecksB).Size = new Size(173, 47);
		((Control)ChecksB).TabIndex = 4;
		((ButtonBase)ChecksB).Text = "Зміни та чеки";
		((ButtonBase)ChecksB).UseVisualStyleBackColor = true;
		((Control)ReportsB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ReportsB).Location = new Point(514, 83);
		((Control)ReportsB).Name = "ReportsB";
		((Control)ReportsB).Size = new Size(173, 47);
		((Control)ReportsB).TabIndex = 5;
		((ButtonBase)ReportsB).Text = "Звіти за період";
		((ButtonBase)ReportsB).UseVisualStyleBackColor = true;
		((Control)SearchB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SearchB).Location = new Point(924, 83);
		((Control)SearchB).Name = "SearchB";
		((Control)SearchB).Size = new Size(138, 47);
		((Control)SearchB).TabIndex = 6;
		((ButtonBase)SearchB).Text = "Пошук";
		((ButtonBase)SearchB).UseVisualStyleBackColor = true;
		((Control)SeaT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SeaT).Location = new Point(706, 92);
		((Control)SeaT).Name = "SeaT";
		((Control)SeaT).Size = new Size(205, 30);
		((Control)SeaT).TabIndex = 7;
		SeaT.TextAlign = (HorizontalAlignment)2;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(12, 48);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(107, 20);
		((Control)Label2).TabIndex = 9;
		Label2.Text = "Організація";
		((Control)IndiT).Enabled = false;
		((Control)IndiT).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)IndiT).Location = new Point(1068, 41);
		((Control)IndiT).Name = "IndiT";
		((TextBoxBase)IndiT).ReadOnly = true;
		((Control)IndiT).Size = new Size(49, 34);
		((Control)IndiT).TabIndex = 10;
		IndiT.TextAlign = (HorizontalAlignment)2;
		((TextBoxBase)DateUpdadeT).BackColor = SystemColors.Window;
		((Control)DateUpdadeT).Enabled = false;
		((Control)DateUpdadeT).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)DateUpdadeT).Location = new Point(155, 83);
		DateUpdadeT.Multiline = true;
		((Control)DateUpdadeT).Name = "DateUpdadeT";
		((TextBoxBase)DateUpdadeT).ReadOnly = true;
		((Control)DateUpdadeT).Size = new Size(174, 47);
		((Control)DateUpdadeT).TabIndex = 11;
		DateUpdadeT.TextAlign = (HorizontalAlignment)2;
		((Control)NoSB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoSB).Location = new Point(1068, 83);
		((Control)NoSB).Name = "NoSB";
		((Control)NoSB).Size = new Size(49, 47);
		((Control)NoSB).TabIndex = 12;
		((ButtonBase)NoSB).Text = "X";
		((ButtonBase)NoSB).UseVisualStyleBackColor = true;
		((ToolStripItem)ToolStripMenuItem2).Name = "ToolStripMenuItem2";
		((ToolStripItem)ToolStripMenuItem2).Size = new Size(291, 6);
		((ToolStripItem)ДоступніЛіцензіїToolStripMenuItem).Name = "ДоступніЛіцензіїToolStripMenuItem";
		((ToolStripItem)ДоступніЛіцензіїToolStripMenuItem).Size = new Size(294, 26);
		((ToolStripItem)ДоступніЛіцензіїToolStripMenuItem).Text = "Доступні ліцензії...";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1129, 595);
		((Control)this).Controls.Add((Control)(object)NoSB);
		((Control)this).Controls.Add((Control)(object)DateUpdadeT);
		((Control)this).Controls.Add((Control)(object)IndiT);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)SeaT);
		((Control)this).Controls.Add((Control)(object)SearchB);
		((Control)this).Controls.Add((Control)(object)ReportsB);
		((Control)this).Controls.Add((Control)(object)ChecksB);
		((Control)this).Controls.Add((Control)(object)UpdateB);
		((Control)this).Controls.Add((Control)(object)LBfn);
		((Control)this).Controls.Add((Control)(object)CBorg);
		((Control)this).Controls.Add((Control)(object)MenuStrip1);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MainMenuStrip = MenuStrip1;
		((Form)this).MaximizeBox = false;
		((Control)this).Name = "FormAccountant";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Перегляд підприємств";
		((Control)MenuStrip1).ResumeLayout(false);
		((Control)MenuStrip1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormAccountant_Load(object sender, EventArgs e)
	{
		((Form)this).Text = ((Form)this).Text + "           ( WebCheck0.dll  v.6.0.7.1331 )";
		((Control)UpdateB).Enabled = false;
		((Control)ChecksB).Enabled = false;
		((Control)ReportsB).Enabled = false;
		((Control)SearchB).Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		((Control)NoSB).Enabled = ((Control)SearchB).Enabled;
		OrgS();
	}

	private void ДодатиПідприємствоToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		((ObjectCollection)LBfn.Items).Clear();
		All.TinBux = "";
		((Form)new FormAddTin()).ShowDialog();
		OrgS();
		if (All.TinBux.Length > 1)
		{
			CBorg.Text = All.TinBux;
		}
	}

	private void CBorg_SelectedIndexChanged(object sender, EventArgs e)
	{
		fullD = false;
		((Control)UpdateB).Enabled = false;
		((Control)ChecksB).Enabled = false;
		((Control)ReportsB).Enabled = false;
		((Control)SearchB).Enabled = true;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		((Control)NoSB).Enabled = ((Control)SearchB).Enabled;
		SeaT.Text = "";
		DateUpdadeT.Text = "";
		string text = AO.TextToNumber(CBorg.Text);
		FnS(text);
		string fillePath = All.MyDoc() + "\\WebCheck\\Archive\\All\\" + AO.NameFileTIN(text) + ".p7s";
		string text2 = AO.SendFile(fillePath);
		if (Operators.CompareString(text2, "", false) != 0 && AO.Dereban(text2))
		{
			fullD = true;
		}
		if (fullD)
		{
			((TextBoxBase)IndiT).BackColor = Color.LightGreen;
		}
		else
		{
			((TextBoxBase)IndiT).BackColor = Color.LightPink;
		}
	}

	private void OrgS()
	{
		ObjectCollection items = CBorg.Items;
		items.Clear();
		int num = All.ArS.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			string text = All.ArS.NameFn(i);
			items.Add((object)(text + "   -   " + All.ArS.StringGetFn(text, "OrgName")));
		}
		items = null;
	}

	private void FnS(string eT)
	{
		ObjectCollection items = LBfn.Items;
		((ObjectCollection)items).Clear();
		checked
		{
			int num = AO.CounterKeysTin(eT) - 1;
			for (int i = 0; i <= num; i++)
			{
				string text = All.ArS.StringGetFn(eT, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString()))) + "    ";
				text += NameL(All.ArS.StringGetFn(eT, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString()))));
				text = text + "   " + All.ArS.StringGetFn(eT, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())));
				((ObjectCollection)items).Add((object)text);
			}
			items = null;
		}
	}

	private void FnSeaS(string eT)
	{
		ObjectCollection items = LBfn.Items;
		((ObjectCollection)items).Clear();
		checked
		{
			int num = AO.CounterKeysTin(eT) - 1;
			for (int i = 0; i <= num; i++)
			{
				string text = All.ArS.StringGetFn(eT, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString()))) + "    ";
				text += NameL(All.ArS.StringGetFn(eT, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString()))));
				text = text + "   " + All.ArS.StringGetFn(eT, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())));
				if (text.ToLower().IndexOf(SeaT.Text.ToLower()) > -1)
				{
					((ObjectCollection)items).Add((object)text);
				}
			}
			items = null;
		}
	}

	private string NameL(string eN, int eL = 25)
	{
		if (eN.Length > eL)
		{
			return eN.Substring(0, eL);
		}
		if (eN.Length < eL)
		{
			eN += Strings.Space(checked((int)Math.Round((double)(eL - eN.Length) * 1.9)));
		}
		return eN;
	}

	private void LBfn_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			int num = ((ObjectCollection)LBfn.Items).Count - 1;
			for (int i = 0; i <= num; i++)
			{
				LBfn.SetItemChecked(i, false);
				try
				{
					LBfn.SetItemChecked(((ListBox)LBfn).SelectedIndex, true);
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					((Control)UpdateB).Enabled = false;
					((Control)ChecksB).Enabled = false;
					((Control)ReportsB).Enabled = false;
					ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
					ProjectData.ClearProjectError();
					return;
				}
			}
			if (Operators.ConditionalCompareObjectNotEqual(((ListBox)LBfn).SelectedItem, (object)null, false))
			{
				CopyLocalBase(((ListBox)LBfn).SelectedItem.ToString());
			}
			string text = AO.TextToNumber(((ListBox)LBfn).SelectedItem.ToString());
			if (File.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db"))
			{
				((Control)ChecksB).Enabled = true;
				((Control)ReportsB).Enabled = true;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = true;
			}
			else
			{
				((Control)ChecksB).Enabled = false;
				((Control)ReportsB).Enabled = false;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
			}
			((Control)UpdateB).Enabled = true;
			((Control)SearchB).Enabled = true;
			((Control)NoSB).Enabled = ((Control)SearchB).Enabled;
		}
	}

	private string ShowReortDate(string ef)
	{
		//IL_00b9: Unknown result type (might be due to invalid IL or missing references)
		ef = AO.TextToNumber(ef);
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + ef + "\\" + ef + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = ef;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject((object)"Data Source=", obj), (object)"; Version=3"));
			All.PayTax.StartLoad();
			FormDate formDate = new FormDate();
			((Form)formDate).ShowDialog();
			((Component)(object)formDate).Dispose();
			All.A.Connection = "";
			All.A.Status = false;
			return "";
		}
		All.A.FN = "";
		All.A.Status = false;
		return "ошибка";
	}

	private string ShowReportsFN(string eF)
	{
		//IL_00c4: Unknown result type (might be due to invalid IL or missing references)
		eF = AO.TextToNumber(eF);
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + eF + "\\" + eF + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = eF;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject((object)"Data Source=", obj), (object)"; Version=3"));
			All.PayTax.StartLoad();
			All.A.Status = true;
			FormReports formReports = new FormReports();
			((Form)formReports).ShowDialog();
			((Component)(object)formReports).Dispose();
			All.A.Connection = "";
			All.A.Status = false;
			return "";
		}
		All.A.FN = "";
		All.A.Status = false;
		return "ошибка";
	}

	private bool CopyLocalBase(string eF)
	{
		eF = AO.TextToNumber(eF);
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + eF + ".db";
		string text2 = All.MyDoc() + "\\WebCheck\\Archive\\" + eF + "\\" + eF + ".db";
		if (File.Exists(text))
		{
			((ServerComputer)MyProject.Computer).FileSystem.CopyFile(text, text2, true);
			UpdateTime(AO.TextToNumber(CBorg.Text), eF);
			DateUpdadeT.Text = GetUpdateTime(AO.TextToNumber(CBorg.Text), eF);
			return true;
		}
		DateUpdadeT.Text = GetUpdateTime(AO.TextToNumber(CBorg.Text), eF);
		return false;
	}

	private void ChecksB_Click(object sender, EventArgs e)
	{
		ShowReportsFN(((ListBox)LBfn).SelectedItem.ToString());
	}

	private void ReportsB_Click(object sender, EventArgs e)
	{
		ShowReortDate(((ListBox)LBfn).SelectedItem.ToString());
	}

	private void UpdateB_Click(object sender, EventArgs e)
	{
		//IL_0082: Unknown result type (might be due to invalid IL or missing references)
		if (!CopyLocalBase(((ListBox)LBfn).SelectedItem.ToString()))
		{
			TypLoadDB e2 = default(TypLoadDB);
			e2.Tin = AO.TextToNumber(CBorg.Text);
			e2.FN = AO.TextToNumber(((ListBox)LBfn).Text);
			e2.Update = GetUpdateTime(e2.Tin, e2.FN);
			e2.Online = fullD;
			FormLoadDB formLoadDB = new FormLoadDB(e2);
			((Form)formLoadDB).ShowDialog();
			((Component)(object)formLoadDB).Dispose();
			DateUpdadeT.Text = GetUpdateTime(e2.Tin, e2.FN);
			string text = AO.TextToNumber(((ListBox)LBfn).SelectedItem.ToString());
			if (File.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db"))
			{
				((Control)ChecksB).Enabled = true;
				((Control)ReportsB).Enabled = true;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = true;
			}
			else
			{
				((Control)ChecksB).Enabled = false;
				((Control)ReportsB).Enabled = false;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
			}
			((Control)UpdateB).Enabled = true;
			((Control)SearchB).Enabled = true;
			((Control)NoSB).Enabled = ((Control)SearchB).Enabled;
		}
	}

	private void UpdateTime(string eTin, string eFn)
	{
		int num = AO.IndexKeysTin(eTin, eFn);
		All.ArS.StringWriteFN(eTin, AO.NameKeyINI("UP", Conversions.ToInteger(num.ToString())), "L " + DateTime.Now);
	}

	private string GetUpdateTime(string eTin, string eFn)
	{
		int num = AO.IndexKeysTin(eTin, eFn);
		string text = All.ArS.StringGetFn(eTin, AO.NameKeyINI("UP", Conversions.ToInteger(num.ToString())));
		if (Operators.CompareString(text, "", false) == 0)
		{
			text = "no update";
		}
		return text;
	}

	private void SearchB_Click(object sender, EventArgs e)
	{
		((Control)SearchB).Enabled = false;
		string eT = AO.TextToNumber(CBorg.Text);
		if (Operators.CompareString(SeaT.Text.Trim(), "", false) == 0)
		{
			FnS(eT);
			((Control)SearchB).Enabled = true;
			return;
		}
		FnSeaS(eT);
		((Control)UpdateB).Enabled = false;
		((Control)ChecksB).Enabled = false;
		((Control)ReportsB).Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		((Control)SearchB).Enabled = true;
	}

	private void SeaT_MouseDoubleClick(object sender, MouseEventArgs e)
	{
		SeaT.Text = "";
	}

	private void NoSB_Click(object sender, EventArgs e)
	{
		((Control)NoSB).Enabled = false;
		SeaT.Text = "";
		string eT = AO.TextToNumber(CBorg.Text);
		FnS(eT);
		((Control)UpdateB).Enabled = false;
		((Control)ChecksB).Enabled = false;
		((Control)ReportsB).Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		((Control)NoSB).Enabled = true;
	}

	private void ОновитиДаніПідприємстваToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		((ObjectCollection)LBfn.Items).Clear();
		All.TinBux = "";
		((Form)new FormAddTin(e: false)).ShowDialog();
		OrgS();
		if (All.TinBux.Length > 1)
		{
			CBorg.Text = All.TinBux;
		}
	}

	private void ЕкспортЧеківЗаПеріодToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_00db: Unknown result type (might be due to invalid IL or missing references)
		string text = AO.TextToNumber(((ListBox)LBfn).SelectedItem.ToString());
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = text;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject((object)"Data Source=", obj), (object)"; Version=3"));
			All.PayTax.StartLoad();
			All.A.Status = true;
			ExportingToText exportingToText = new ExportingToText(text);
			((Form)exportingToText).ShowDialog();
			((Component)(object)exportingToText).Dispose();
			All.A.Connection = "";
			All.A.Status = false;
		}
		else
		{
			All.A.FN = "";
			All.A.Status = false;
			All.A.Connection = "";
		}
	}

	private void ДоступніЛіцензіїToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_000a: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormLicenses()).ShowDialog();
	}
}
